//! Core data model of a single sheet plus the dependency-tracking /
//! recalculation engine.
//!
//! Everything in this module is plain Rust (no GUI, no I/O) so it is shared
//! unchanged by the desktop app, the terminal CLI and the wasm bridge.

use crate::calculate_functions::compute_cell;
use crate::calculate_functions::compute_range_func;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::string::String;

/// Serde helper: `HashMap<(i16, i16), T>` as a list of `[[row, col], value]` pairs
/// (JSON object keys must be strings, tuples are not).
pub mod tuple_key_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S, T>(map: &HashMap<(i16, i16), T>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        let vec: Vec<_> = map.iter().map(|(&(r, c), v)| ((r, c), v)).collect();
        vec.serialize(serializer)
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<HashMap<(i16, i16), T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        let vec: Vec<((i16, i16), T)> = Deserialize::deserialize(deserializer)?;
        Ok(vec.into_iter().collect())
    }
}

/// Serde helper for the range-dependency map (`tuup`).
pub mod nested_tuple_key_map {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S, T>(
        map: &HashMap<((i16, i16), (i16, i16)), T>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        let vec: Vec<_> = map.iter().map(|(&(k1, k2), v)| (((k1, k2)), v)).collect();
        vec.serialize(serializer)
    }

    pub fn deserialize<'de, D, T>(
        deserializer: D,
    ) -> Result<HashMap<((i16, i16), (i16, i16)), T>, D::Error>
    where
        D: Deserializer<'de>,
        T: Deserialize<'de>,
    {
        let vec: Vec<(((i16, i16), (i16, i16)), T)> = Deserialize::deserialize(deserializer)?;
        Ok(vec.into_iter().collect())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpCode {
    NoConstraint,
    CellEqualsCell,
    CellPlusCell,
    CellMinusCell,
    CellTimesCell,
    CellDivideCell,
    CellPlusConstant,
    CellMinusConstant,
    ConstantMinusCell,
    CellTimesConstant,
    CellDivideConstant,
    ConstantDividesCell,
    Sum,
    Min,
    Max,
    Avg,
    Stdev,
    Sleep,
    String,
}

/// A (row, col) reference into a sheet. `(-1, -1)` means "no reference".
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CellInfo {
    pub row: i16,
    pub col: i16,
}

impl CellInfo {
    /// The "no reference" sentinel used by `cell1` / `cell2`.
    pub const NONE: CellInfo = CellInfo { row: -1, col: -1 };

    pub fn is_none(&self) -> bool {
        self.row == -1 && self.col == -1
    }
}

/// Maximum sheet size supported by the engine. Dependency keys are encoded as
/// `col * 1000 + row`, so rows must stay below 1000; columns are capped at
/// `ZZZ` (three letters).
pub const MAX_ROWS: i32 = 1000;
pub const MAX_COLS: i32 = 18278;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub value: i32,
    pub is_error: bool,
    pub string: Option<String>,
    pub op_code: OpCode,
    pub cell1: CellInfo,
    pub cell2: CellInfo,
    /// Keys (`col * 1000 + row`) of the cells whose formula references this cell.
    pub dependencies: HashSet<i32>,
}

impl Default for Cell {
    fn default() -> Self {
        Cell::empty()
    }
}

impl Cell {
    /// A blank cell: value 0, no formula, no dependents.
    pub fn empty() -> Self {
        Cell {
            value: 0,
            is_error: false,
            string: None,
            op_code: OpCode::NoConstraint,
            cell1: CellInfo::NONE,
            cell2: CellInfo::NONE,
            dependencies: HashSet::new(),
        }
    }

    /// The text a UI should show for this cell: `Err` for error cells, the
    /// string contents for string cells, otherwise the numeric value.
    ///
    /// This is the single shared definition used by the CLI printer, the CSV
    /// exporter, the desktop grid and the wasm bridge.
    pub fn display_value(&self) -> String {
        if self.is_error {
            "Err".to_string()
        } else if let Some(s) = &self.string {
            s.clone()
        } else {
            self.value.to_string()
        }
    }

    /// `true` if this cell currently holds a formula (anything that is not a
    /// plain constant / string).
    pub fn has_formula(&self) -> bool {
        !matches!(self.op_code, OpCode::NoConstraint | OpCode::String)
    }
}

/// Serde helper for `Sheet::buul`.
///
/// Written as `{"rows":R,"cols":C,"set":[[row,col],...]}` listing only the
/// `true` cells, which keeps a blank 1000x1000 sheet at a few bytes instead of
/// ~6 MB. Reading also accepts the original full `[[bool,...],...]` grid, so
/// `.290` files written by the old desktop build still load.
pub mod range_flags {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Compact {
        rows: usize,
        cols: usize,
        set: Vec<(u16, u16)>,
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        Grid(Vec<Vec<bool>>),
        Compact(Compact),
    }

    pub fn serialize<S: Serializer>(grid: &[Vec<bool>], serializer: S) -> Result<S::Ok, S::Error> {
        let rows = grid.len();
        let cols = grid.first().map(|r| r.len()).unwrap_or(0);
        let mut set = Vec::new();
        for (r, row) in grid.iter().enumerate() {
            for (c, &flag) in row.iter().enumerate() {
                if flag {
                    set.push((r as u16, c as u16));
                }
            }
        }
        Compact { rows, cols, set }.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Vec<bool>>, D::Error> {
        match Either::deserialize(deserializer)? {
            Either::Grid(grid) => Ok(grid),
            Either::Compact(c) => {
                let mut grid = vec![vec![false; c.cols]; c.rows];
                for (r, col) in c.set {
                    if let Some(row) = grid.get_mut(r as usize) {
                        if let Some(flag) = row.get_mut(col as usize) {
                            *flag = true;
                        }
                    }
                }
                Ok(grid)
            }
        }
    }
}

/// Horizontal alignment of a cell's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Visual formatting of one cell. Pure presentation: never influences a
/// value, a formula or the dependency graph. Stored per cell in
/// `Sheet::formats` so it is saved with the workbook; only non-default
/// fields are written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellFormat {
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    /// Font size in CSS pixels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size: Option<u8>,
    /// Text colour as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Background colour as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl CellFormat {
    pub fn is_default(&self) -> bool {
        *self == CellFormat::default()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Sheet {
    pub rows: i32,
    pub cols: i32,
    /// `buul[row][col]` is `true` when the cell is covered by at least one
    /// range function (SUM/MIN/MAX/AVG/STDEV) somewhere in the sheet.
    #[serde(with = "range_flags")]
    pub buul: Vec<Vec<bool>>,
    /// Sparse cell storage keyed by `(row, col)`.
    #[serde(with = "tuple_key_map")]
    pub data: HashMap<(i16, i16), Cell>,
    /// Range dependencies: `((col1,row1),(col2,row2)) -> [(col,row) of cells
    /// whose formula is a range function over that rectangle]`.
    #[serde(with = "nested_tuple_key_map")]
    #[allow(clippy::type_complexity)]
    pub tuup: HashMap<((i16, i16), (i16, i16)), Vec<(i16, i16)>>,
    /// Per-cell visual formatting (absent = default). Missing in files
    /// written before formatting existed, hence `default`.
    #[serde(default, with = "tuple_key_map")]
    pub formats: HashMap<(i16, i16), CellFormat>,
    /// Charts defined on this sheet (definitions only; the data is read from
    /// the cells when a chart is rendered).
    #[serde(default)]
    pub charts: Vec<crate::charts::ChartSpec>,
}

/// Retrieves or creates a cell at the specified row and column in the sheet.
///
/// # Arguments
/// * `sheet` - The mutable spreadsheet to operate on.
/// * `row` - The row index of the cell.
/// * `col` - The column index of the cell.
///
/// # Returns
/// A mutable reference to the cell.
pub fn get_or_create_cell(sheet: &mut Sheet, row: i32, col: i32) -> &mut Cell {
    sheet
        .data
        .entry((row as i16, col as i16))
        .or_insert_with(Cell::empty)
}
// is_valid_cell
/// Checks if a cell is within the valid bounds of the spreadsheet.
///
/// # Arguments
/// * `row` - The row index to check.
/// * `col` - The column index to check.
/// * `total_rows` - The total number of rows in the spreadsheet.
/// * `total_cols` - The total number of columns in the spreadsheet.
///
/// # Returns
/// `true` if the cell is within bounds, `false` otherwise.
pub fn is_valid_cell(row: i32, col: i32, total_rows: i32, total_cols: i32) -> bool {
    row >= 0 && row < total_rows && col >= 0 && col < total_cols
}

impl Sheet {
    /// Creates a new spreadsheet with the specified dimensions.
    ///
    /// # Arguments
    /// * `rows` - The number of rows in the spreadsheet.
    /// * `cols` - The number of columns in the spreadsheet.
    ///
    /// # Returns
    /// A new `Sheet` instance.
    pub fn new(rows: i32, cols: i32) -> Self {
        let buul = vec![vec![false; cols as usize]; rows as usize];

        // Initialize the `buul` vector

        // Initialize the `data` and `tuup` HashMaps as empty
        let data = HashMap::with_capacity(1000);
        let tuup = HashMap::with_capacity(1000);

        Sheet {
            rows,
            cols,
            buul,
            data,
            tuup,
            formats: HashMap::new(),
            charts: Vec::new(),
        }
    }
}

/// Converts a column number to its corresponding alphabetical column name (e.g., 0 -> "A", 1 -> "B").
///
/// # Arguments
/// * `col_num` - The column number (0-based).
///
/// # Returns
/// The alphabetical column name as a `String`.
pub fn col_num_to_col_name(col_num: i32) -> String {
    let mut col_name = String::new();
    let mut col_num = col_num;
    while col_num >= 0 {
        let x = (b'A' + (col_num % 26) as u8) as char;
        col_name.push(x);
        col_num /= 26;
        col_num -= 1;
    }
    col_name = col_name.chars().rev().collect();
    col_name
}
/// Converts an alphabetical column name to its corresponding column number (e.g., "A" -> 0, "B" -> 1).
///
/// # Arguments
/// * `col_name` - The alphabetical column name.
///
/// # Returns
/// The column number (0-based).
pub fn col_name_to_col_num(col_name: &str) -> i32 {
    let mut col = -1;
    for c in col_name.chars() {
        col = (col + 1) * 26;
        col += (c as u8 - b'A') as i32;
    }
    col
}
/// Renders a 10x10 window of the sheet as fixed-width text (the terminal CLI
/// view). Pure function: returns the text instead of printing it.
///
/// # Arguments
/// * `start_row` - The starting row index.
/// * `start_col` - The starting column index.
/// * `total_rows` - The total number of rows in the spreadsheet.
/// * `total_cols` - The total number of columns in the spreadsheet.
/// * `sheet` - The spreadsheet to render.
pub fn render_sheet_text(
    start_row: i32,
    start_col: i32,
    total_rows: i32,
    total_cols: i32,
    sheet: &Sheet,
) -> String {
    let max_col_display: i32 = start_col + std::cmp::min(total_cols - start_col, 10);
    let max_row_display: i32 = start_row + std::cmp::min(total_rows - start_row, 10);
    let space: usize = 10;
    let mut out = String::new();

    // Column headers
    out.push_str(&format!("{:>space$}", ""));
    for i in start_col..max_col_display {
        out.push_str(&format!("{:>space$}", col_num_to_col_name(i)));
    }
    out.push('\n');

    // Rows
    for i in start_row..max_row_display {
        out.push_str(&format!("{:>space$}", i + 1)); // row header
        for j in start_col..max_col_display {
            match sheet.data.get(&(i as i16, j as i16)) {
                Some(cell) => out.push_str(&format!("{:>space$}", cell.display_value())),
                // Cells that were never touched are implicitly 0
                None => out.push_str(&format!("{:>space$}", 0)),
            }
        }
        out.push('\n');
    }
    out
}

/// Prints a portion of the spreadsheet to the console, starting from the specified coordinates.
/// Thin stdout wrapper around [`render_sheet_text`].
pub fn print_sheet(
    start_row: i32,
    start_col: i32,
    total_rows: i32,
    total_cols: i32,
    sheet: &mut Sheet,
) {
    print!(
        "{}",
        render_sheet_text(start_row, start_col, total_rows, total_cols, sheet)
    );
}
/// Checks for circular dependencies between two cells.
///
/// # Arguments
/// * `avl_tree` - A HashMap tracking cell dependencies and their indegrees.
/// * `cell1` - The first cell to check.
/// * `cell2` - The second cell to check.
///
/// # Returns
/// `true` if a cycle is detected, `false` otherwise.
pub fn check_cycle(
    avl_tree: &std::collections::HashMap<i32, i32>,
    cell1: &CellInfo,
    cell2: &CellInfo,
) -> bool {
    let key1 = cell1.col as i32 * 1000 + cell1.row as i32;
    if avl_tree.contains_key(&key1) {
        return true;
    }
    if cell2.col == -1 && cell2.row == -1 {
        return false;
    }
    let key2 = cell2.col as i32 * 1000 + cell2.row as i32;
    if avl_tree.contains_key(&key2) {
        return true;
    }
    false
}

/// Checks for circular dependencies in range-based functions.
///
/// # Arguments
/// * `avl_tree` - A HashMap tracking cell dependencies and their indegrees.
/// * `cell1` - The top-left cell of the range.
/// * `cell2` - The bottom-right cell of the range.
///
/// # Returns
/// `true` if a cycle is detected, `false` otherwise.
pub fn check_cycle_range_funcs(
    avl_tree: &std::collections::HashMap<i32, i32>,
    cell1: &CellInfo,
    cell2: &CellInfo,
) -> bool {
    for (&key, _) in avl_tree.iter() {
        let col = key / 1000;
        let row = key % 1000;

        if col >= cell1.col as i32
            && col <= cell2.col as i32
            && row >= cell1.row as i32
            && row <= cell2.row as i32
        {
            return true;
        }
    }
    false
}
/// Recalculates the value of a cell based on its operation and dependencies.
///
/// # Arguments
/// * `sheet` - The mutable spreadsheet to operate on.
/// * `row` - The row index of the cell.
/// * `col` - The column index of the cell.
/// * `sleep_timer` - A mutable reference to track accumulated sleep time.
pub fn recalculate(sheet: &mut Sheet, row: usize, col: usize, sleep_timer: &mut i32) {
    let (op_code, cell1, cell2) = {
        let c = sheet
            .data
            .get_mut(&(row as i16, col as i16))
            .expect("not in map");
        (c.op_code, c.cell1.clone(), c.cell2.clone())
    };
    if op_code == NoConstraint || op_code == String {
        return;
    }

    // ── 2) Compute the new value, error flag, and any sleep delta using *only* immutable borrows ──
    let (new_value, string_value, is_error, is_error2) = {
        // Immutable borrow of the whole sheet
        let mut err = false;
        let mut val = 0;
        let mut strval = String::new();
        let mut err1: bool = false;

        match op_code {
            CellEqualsCell => {
                let ref_cell = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
                if ref_cell.is_error {
                    err = true;
                } else if ref_cell.string.is_some() {
                    strval = ref_cell.string.as_ref().unwrap().clone();
                } else {
                    val = ref_cell.value;
                }
            }
            CellPlusCell | CellMinusCell | CellTimesCell | CellDivideCell => {
                let (a_value, a_is_error, a_string) = {
                    let a = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
                    (a.value, a.is_error, a.string.clone())
                };
                let (b_value, b_is_error, b_string) = {
                    let b = get_or_create_cell(sheet, cell2.row as i32, cell2.col as i32);
                    (b.value, b.is_error, b.string.clone())
                };
                if a_is_error || b_is_error || a_string.is_some() || b_string.is_some() {
                    err = true;
                } else {
                    (val, err1) = compute_cell(op_code, a_value, b_value, &mut String::new());
                }
            }
            CellPlusConstant | CellMinusConstant | CellTimesConstant | CellDivideConstant
            | ConstantDividesCell | ConstantMinusCell => {
                let a = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
                if a.is_error || a.string.is_some() {
                    err = true;
                } else {
                    (val, err1) = compute_cell(
                        op_code,
                        a.value,
                        (cell2.row as i32) << 16 | cell2.col as i32,
                        &mut String::new(),
                    );
                }
            }
            Sleep => {
                let a = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
                if a.is_error || a.string.is_some() {
                    err = true;
                } else {
                    val = a.value;
                    *sleep_timer += val;
                }
            }
            Sum | Min | Max | Avg | Stdev => {
                // range check
                for i in cell1.row..=cell2.row {
                    for j in cell1.col..=cell2.col {
                        if let Some(ref_cell) = sheet.data.get(&(i, j)) {
                            if ref_cell.is_error || ref_cell.string.is_some() {
                                err = true;
                                break;
                            }
                        }
                    }
                    if err {
                        break;
                    }
                }

                if !err {
                    val = compute_range_func(
                        sheet,
                        op_code,
                        cell1.row,
                        cell1.col,
                        cell2.row,
                        cell2.col,
                        &mut String::new(),
                    );
                }
            }
            _ => {}
        }
        (val, strval, err, err1)
    };
    {
        let c = get_or_create_cell(sheet, row as i32, col as i32);
        c.is_error = is_error;
        if is_error2 {
            c.is_error = true;
        } else if op_code == CellEqualsCell && !string_value.is_empty() {
            c.string = Some(string_value.clone());
        } else {
            c.value = new_value;
            c.string = None;
        }
    }
}

/// Performs a topological sort on the dependency graph to determine the order of cell recalculation.
///
/// # Arguments
/// * `avl_tree` - A HashMap tracking cell dependencies and their indegrees.
/// * `sheet` - The spreadsheet containing the cell data.
///
/// # Returns
/// A vector of cell keys in topological order.
pub fn topological_sort(
    avl_tree: &mut std::collections::HashMap<i32, i32>,
    sheet: &Sheet,
) -> Vec<i32> {
    let mut queue = std::collections::VecDeque::new();
    let mut result = Vec::new();

    // Find nodes with indegree 0
    for (&key, &value) in avl_tree.iter() {
        if value == 0 {
            queue.push_back(key);
        }
    }

    while let Some(node) = queue.pop_front() {
        result.push(node);

        for &dep in &sheet
            .data
            .get(&((node % 1000) as i16, (node / 1000) as i16))
            .expect("not in map")
            .dependencies
        {
            if let Some(indegree) = avl_tree.get_mut(&dep) {
                *indegree -= 1;
                if *indegree == 0 {
                    queue.push_back(dep);
                }
            }
        }
        // get the range dependencies
        let (row, col) = (node % 1000, node / 1000);
        if sheet.buul[row as usize][col as usize] {
            for (((start_col, start_row), (end_col, end_row)), target_vector) in sheet.tuup.iter() {
                for (target_col, target_row) in target_vector {
                    if col >= *start_col as i32
                        && col <= *end_col as i32
                        && row >= *start_row as i32
                        && row <= *end_row as i32
                    {
                        let key = *target_col as i32 * 1000 + *target_row as i32;
                        if let Some(indegree) = avl_tree.get_mut(&key) {
                            *indegree -= 1;
                            if *indegree == 0 {
                                queue.push_back(key);
                            }
                        }
                    }
                }
            }
        }
    }
    result
}

use OpCode::*;

/// Removes dependencies for a specified cell, updating dependent cells and ranges.
///
/// # Arguments
/// * `cell` - The cell whose dependencies should be removed.
/// * `sheet` - The mutable spreadsheet to operate on.
pub fn remove_dependency(cell: &CellInfo, sheet: &mut Sheet) {
    let row = cell.row as i32;
    let col = cell.col as i32;

    // Check if the current cell exists in the map
    let (op_code, cell1, cell2) = if let Some(curr_cell) = sheet.data.get(&(row as i16, col as i16))
    {
        (
            curr_cell.op_code,
            curr_cell.cell1.clone(),
            curr_cell.cell2.clone(),
        )
    } else {
        // If the cell is not in the map, return early
        return;
    };

    match op_code {
        OpCode::NoConstraint => {}
        OpCode::CellEqualsCell
        | OpCode::CellPlusConstant
        | OpCode::CellTimesConstant
        | OpCode::CellMinusConstant
        | OpCode::CellDivideConstant
        | OpCode::ConstantDividesCell
        | OpCode::Sleep
        | OpCode::ConstantMinusCell => {
            if let Some(dependent_cell) = sheet.data.get_mut(&(cell1.row, cell1.col)) {
                dependent_cell.dependencies.remove(&(col * 1000 + row));
            }
        }
        OpCode::Sum | OpCode::Min | OpCode::Max | OpCode::Avg | OpCode::Stdev => {
            // Handle range dependencies
            if let Some(vec) = sheet
                .tuup
                .get_mut(&((cell1.col, cell1.row), (cell2.col, cell2.row)))
            {
                vec.retain(|&(col, row)| col != cell.col || row != cell.row);
                if vec.is_empty() {
                    sheet
                        .tuup
                        .remove(&((cell1.col, cell1.row), (cell2.col, cell2.row)));
                }
            }
            for i in cell1.row..=cell2.row {
                for j in cell1.col..=cell2.col {
                    let mut flag = false;
                    // check present in other range functions
                    for (((start_col, start_row), (end_col, end_row)), _) in sheet.tuup.iter() {
                        if j >= *start_col && j <= *end_col && i >= *start_row && i <= *end_row {
                            flag = true;
                        }
                    }
                    sheet.buul[i as usize][j as usize] = flag; // Mark the cell as not used
                }
            }
        }
        OpCode::CellPlusCell
        | OpCode::CellMinusCell
        | OpCode::CellTimesCell
        | OpCode::CellDivideCell => {
            if let Some(dependent_cell1) = sheet.data.get_mut(&(cell1.row, cell1.col)) {
                dependent_cell1.dependencies.remove(&(col * 1000 + row));
            }

            if let Some(dependent_cell2) = sheet.data.get_mut(&(cell2.row, cell2.col)) {
                dependent_cell2.dependencies.remove(&(col * 1000 + row));
            }
        }
        _ => {}
    }
    // Update the current cell to reset its dependencies
    if let Some(curr_cell) = sheet.data.get_mut(&(row as i16, col as i16)) {
        curr_cell.cell1 = CellInfo { row: -1, col: -1 };
        curr_cell.cell2 = CellInfo { row: -1, col: -1 };
    }
}

/// Adds a cell and its dependencies to the dependency tree for cycle detection and topological sorting.
///
/// # Arguments
/// * `avl_tree` - The HashMap tracking cell dependencies and their indegrees.
/// * `cell` - The cell to add to the tree.
/// * `sheet` - The spreadsheet containing the cell data.
pub fn add_to_tree(
    avl_tree: &mut std::collections::HashMap<i32, i32>,
    cell: CellInfo,
    sheet: &Sheet,
) {
    // Get the current cell from the HashMap
    if let Some(curr_cell) = sheet.data.get(&(cell.row, cell.col)) {
        for &curr in &curr_cell.dependencies {
            if let std::collections::hash_map::Entry::Vacant(e) = avl_tree.entry(curr) {
                e.insert(1);
                let temp = CellInfo {
                    row: (curr % 1000) as i16,
                    col: (curr / 1000) as i16,
                };
                add_to_tree(avl_tree, temp, sheet);
            } else {
                *avl_tree.entry(curr).or_insert(1) += 1;
            }
        }
    } // check range dependency
    if sheet.buul[cell.row as usize][cell.col as usize] {
        // range dependency is there
        for (((start_col, start_row), (end_col, end_row)), target_vector) in sheet.tuup.iter() {
            if cell.col >= *start_col
                && cell.col <= *end_col
                && cell.row >= *start_row
                && cell.row <= *end_row
            {
                for (target_col, target_row) in target_vector {
                    // dependent
                    let key = *target_col as i32 * 1000 + *target_row as i32;
                    if let std::collections::hash_map::Entry::Vacant(e) = avl_tree.entry(key) {
                        e.insert(1);
                        let temp = CellInfo {
                            row: *target_row,
                            col: *target_col,
                        };
                        add_to_tree(avl_tree, temp, sheet);
                    } else {
                        *avl_tree.entry(key).or_insert(1) += 1;
                    }
                }
            }
        }
    }
}

/// Adds a new constraint to a cell, updating its operation and dependencies.
///
/// # Arguments
/// * `curr_cell` - The cell to apply the constraint to.
/// * `cell1` - The first cell reference for the operation.
/// * `cell2` - The second cell reference for the operation.
/// * `op_code` - The operation to apply.
/// * `sheet` - The mutable spreadsheet to operate on.
/// * `status` - A mutable string to store the operation status (e.g., "ok" or "circular error").
/// * `sleep_timer` - A mutable reference to track accumulated sleep time.
pub fn add_constraints(
    curr_cell: CellInfo,
    cell1: CellInfo,
    cell2: CellInfo,
    op_code: OpCode,
    sheet: &mut Sheet,
    status: &mut String,
    sleep_timer: &mut i32,
) {
    let key = curr_cell.col as i32 * 1000 + curr_cell.row as i32;
    let mut ans = 0;
    let mut calc_error = false;
    let mut calc_error1 = false;
    let mut stringans = String::new();
    // Create a HashMap named avl_tree where the key is key and the value is indegree (0 by default)
    let mut avl_tree: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    avl_tree.insert(key, 0);
    let temp = CellInfo { row: -1, col: -1 };
    add_to_tree(&mut avl_tree, curr_cell.clone(), sheet);
    match op_code {
        OpCode::NoConstraint | String => {
            remove_dependency(&curr_cell, sheet);
            let cell = get_or_create_cell(sheet, curr_cell.row as i32, curr_cell.col as i32);
            ans = cell.value;
            if cell.is_error {
                calc_error = true;
            }
            cell.op_code = op_code;
            cell.cell1 = CellInfo { row: -1, col: -1 };
            cell.cell2 = CellInfo { row: -1, col: -1 };
        }
        OpCode::CellEqualsCell => {
            if check_cycle(&avl_tree, &cell1, &temp) {
                *status = String::from("circular error");
                return;
            }
            remove_dependency(&curr_cell, sheet);
            let cell = get_or_create_cell(sheet, curr_cell.row as i32, curr_cell.col as i32);
            cell.cell1 = cell1.clone();
            cell.cell2 = CellInfo { row: -1, col: -1 };
            cell.op_code = op_code;
            let ref_cell = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
            match (&ref_cell.string, ref_cell.is_error) {
                (_, true) => calc_error = true,
                (Some(s), false) => stringans = s.clone(),
                (None, false) => ans = ref_cell.value,
            }
            ref_cell.dependencies.insert(key);
        }
        OpCode::Sleep => {
            if check_cycle(&avl_tree, &cell1, &temp) {
                *status = String::from("circular error");
                return;
            }
            remove_dependency(&curr_cell, sheet);
            let cell = get_or_create_cell(sheet, curr_cell.row as i32, curr_cell.col as i32);
            cell.cell1 = cell1.clone();
            cell.cell2 = CellInfo { row: -1, col: -1 };
            cell.op_code = op_code;
            let temp = sheet.rows;
            let temp2 = sheet.cols;
            sheet.rows = temp;
            sheet.cols = temp2;

            let ref_cell = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
            calc_error = ref_cell.is_error || ref_cell.string.is_some();
            if !calc_error {
                ans = ref_cell.value;
                *sleep_timer += ref_cell.value;
            }
            ref_cell.dependencies.insert(key);
        }
        OpCode::CellPlusConstant
        | OpCode::CellMinusConstant
        | OpCode::CellTimesConstant
        | OpCode::CellDivideConstant
        | OpCode::ConstantDividesCell
        | OpCode::ConstantMinusCell => {
            if check_cycle(&avl_tree, &cell1, &temp) {
                *status = String::from("circular error");
                return;
            }
            remove_dependency(&curr_cell, sheet);
            let cell = get_or_create_cell(sheet, curr_cell.row as i32, curr_cell.col as i32);
            cell.cell1 = cell1.clone();
            cell.cell2 = cell2.clone();
            cell.op_code = op_code;

            let value = (cell2.row as i32) << 16 | (cell2.col as i32 & 0xFFFF);
            let ref_cell = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
            calc_error = ref_cell.is_error || ref_cell.string.is_some();
            if !calc_error {
                let (a, b) = compute_cell(op_code, ref_cell.value, value, status);
                ans = a;
                calc_error1 = b;
            }
            ref_cell.dependencies.insert(key);
        }
        OpCode::Sum | OpCode::Min | OpCode::Max | OpCode::Avg | OpCode::Stdev => {
            if check_cycle_range_funcs(&avl_tree, &cell1, &cell2) {
                *status = String::from("circular error");
                return;
            }
            let key1 = (cell1.col, cell1.row);
            let key2 = (cell2.col, cell2.row);
            if sheet.tuup.contains_key(&(key1, key2)) {
                let vec = sheet.tuup.get(&(key1, key2)).unwrap();
                if !vec.is_empty() {
                    let row = vec[0].1;
                    let col = vec[0].0;
                    let cell = get_or_create_cell(sheet, row as i32, col as i32);
                    if cell.is_error || cell.string.is_some() {
                        calc_error = true;
                    }
                }
            } else {
                for i in cell1.row..=cell2.row {
                    for j in cell1.col..=cell2.col {
                        sheet.buul[i as usize][j as usize] = true; // Mark the cell as used
                        if let Some(ref_cell) = sheet.data.get(&(i, j)) {
                            if ref_cell.is_error || ref_cell.string.is_some() {
                                calc_error = true;
                            }
                        }
                    }
                }
            }
            remove_dependency(&curr_cell, sheet);
            if let Some(existing) = sheet
                .tuup
                .get_mut(&((cell1.col, cell1.row), (cell2.col, cell2.row)))
            {
                existing.push((curr_cell.col, curr_cell.row));
            } else {
                sheet.tuup.insert(
                    ((cell1.col, cell1.row), (cell2.col, cell2.row)),
                    vec![(curr_cell.col, curr_cell.row)],
                );
            }
            let cell = get_or_create_cell(sheet, curr_cell.row as i32, curr_cell.col as i32);
            cell.cell1 = cell1.clone();
            cell.cell2 = cell2.clone();
            cell.op_code = op_code;
            ans = compute_range_func(
                sheet, op_code, cell1.row, cell1.col, cell2.row, cell2.col, status,
            );
        }

        OpCode::CellPlusCell
        | OpCode::CellMinusCell
        | OpCode::CellTimesCell
        | OpCode::CellDivideCell => {
            if check_cycle(&avl_tree, &cell1, &cell2) {
                *status = String::from("circular error");
                return;
            }
            remove_dependency(&curr_cell, sheet);
            let cell = get_or_create_cell(sheet, curr_cell.row as i32, curr_cell.col as i32);
            cell.cell1 = cell1.clone();
            cell.cell2 = cell2.clone();
            cell.op_code = op_code;

            let (a_value, a_is_error, a_string, b_value, b_is_error, b_string) = {
                let a = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
                let a_value = a.value;
                let a_is_error = a.is_error;
                let a_string = a.string.clone();

                let b = get_or_create_cell(sheet, cell2.row as i32, cell2.col as i32);
                let b_value = b.value;
                let b_is_error = b.is_error;
                let b_string = b.string.clone();

                (a_value, a_is_error, a_string, b_value, b_is_error, b_string)
            };

            if a_is_error || b_is_error || a_string.is_some() || b_string.is_some() {
                calc_error = true;
            } else {
                let (computed_value, computed_error) =
                    compute_cell(op_code, a_value, b_value, status);
                ans = computed_value;
                calc_error1 = computed_error;
            }

            {
                let a = get_or_create_cell(sheet, cell1.row as i32, cell1.col as i32);
                a.dependencies.insert(key);
            }

            {
                let b = get_or_create_cell(sheet, cell2.row as i32, cell2.col as i32);
                b.dependencies.insert(key);
            }
        }
    }
    let cell = get_or_create_cell(sheet, curr_cell.row as i32, curr_cell.col as i32);
    cell.is_error = calc_error || calc_error1;
    if !cell.is_error && cell.op_code != OpCode::String {
        cell.value = ans;
        cell.string = None;
    }
    if op_code == OpCode::CellEqualsCell && !stringans.is_empty() {
        cell.string = Some(stringans.clone());
    }
    let sorted = topological_sort(&mut avl_tree, sheet);
    *status = String::from("ok");
    for i in sorted.into_iter() {
        let row = i % 1000;
        let col = i / 1000;
        if row == curr_cell.row as i32 && col == curr_cell.col as i32 {
            continue;
        }
        recalculate(sheet, row as usize, col as usize, sleep_timer);
    }
}
