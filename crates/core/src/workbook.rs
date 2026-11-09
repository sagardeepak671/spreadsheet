//! `Workbook`: the host-independent, multi-sheet API on top of the engine.
//!
//! This is the surface the wasm bridge exposes one-to-one to JavaScript. The
//! desktop app still works on `Vec<NamedSheet>` directly (it predates this
//! type) but shares the same `NamedSheet` definition and JSON format, so a
//! `.290` file saved by the desktop loads in the browser and vice versa.

use crate::charts::{ChartData, ChartSpec, chart_data};
use crate::csv;
use crate::dependency_ops::{copy_cell_value, move_cell, sort_order, transitive_dependents};
use crate::formula::{get_cell_formula, get_cell_input};
use crate::parser::{CommandOutcome, Viewport, execute_command};
use crate::sheet_functions::{
    Align, Cell, CellFormat, CellInfo, MAX_COLS, MAX_ROWS, OpCode, Sheet, col_name_to_col_num,
    col_num_to_col_name, is_valid_cell,
};
use regex_lite::Regex;
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;
use std::sync::LazyLock;

/// A sheet plus its tab name. Field names are part of the on-disk JSON
/// (`.290`) format, do not rename them.
#[derive(Clone, Serialize, Deserialize)]
pub struct NamedSheet {
    pub sheet: Sheet,
    pub name: String,
}

/// Errors reported by the `Workbook` API (the engine itself reports problems
/// through status strings such as `"Invalid cmd"` / `"circular error"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    InvalidSheetIndex(usize),
    InvalidCellReference(String),
    CellOutOfBounds {
        reference: String,
        rows: i32,
        cols: i32,
    },
    InvalidDimensions {
        rows: i32,
        cols: i32,
    },
    InvalidChart(u32),
    Json(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::InvalidSheetIndex(i) => write!(f, "no sheet at index {}", i),
            CoreError::InvalidCellReference(r) => write!(f, "invalid cell reference '{}'", r),
            CoreError::CellOutOfBounds {
                reference,
                rows,
                cols,
            } => write!(
                f,
                "cell '{}' is outside the sheet ({} rows x {} cols)",
                reference, rows, cols
            ),
            CoreError::InvalidDimensions { rows, cols } => write!(
                f,
                "invalid sheet size {}x{} (rows 1..={}, cols 1..={})",
                rows, cols, MAX_ROWS, MAX_COLS
            ),
            CoreError::InvalidChart(id) => write!(f, "no chart with id {}", id),
            CoreError::Json(e) => write!(f, "json error: {}", e),
        }
    }
}

impl std::error::Error for CoreError {}

/// Result of applying one edit / command to a sheet.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditResult {
    /// `status == "ok"`.
    pub ok: bool,
    /// Engine status text: `ok`, `Invalid cmd`, `circular error`, `err`.
    pub status: String,
    /// Seconds the command asked to sleep (`SLEEP(..)`); the engine does not
    /// sleep itself.
    pub sleep_seconds: i32,
}

impl EditResult {
    pub fn ok() -> Self {
        EditResult {
            ok: true,
            status: "ok".to_string(),
            sleep_seconds: 0,
        }
    }

    fn from_outcome(status: String, outcome: &CommandOutcome) -> Self {
        EditResult {
            ok: status == "ok",
            status,
            sleep_seconds: outcome.sleep_seconds,
        }
    }
}

/// Read-only view of one (non-empty) cell, convenient for UIs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellSnapshot {
    pub row: i32,
    pub col: i32,
    /// `"A1"` style reference.
    pub reference: String,
    /// What to show in the grid (`Err`, the string, or the number).
    pub display: String,
    pub value: i32,
    pub is_error: bool,
    pub is_string: bool,
    pub has_formula: bool,
    /// Full command that recreates the cell, e.g. `"B2=A1+5"`.
    pub formula: String,
    /// What an editor should show / what the user would type: `A1+5`,
    /// `"hello"`, `5`.
    pub input: String,
    /// Visual formatting, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<CellFormat>,
}

/// Read-only view of one sheet: dimensions plus every non-empty cell
/// (row-major order).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SheetSnapshot {
    pub name: String,
    pub rows: i32,
    pub cols: i32,
    pub cells: Vec<CellSnapshot>,
}

/// One cell's editor text before / after an undoable operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CellChange {
    pub sheet: usize,
    pub reference: String,
    pub before: String,
    pub after: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_format: Option<CellFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_format: Option<CellFormat>,
}

/// An undoable unit of work (one edit, one paste, one sort, ...).
///
/// Undo replays `before` texts and redo replays `after` texts through the
/// engine (`set_cell` / `clear_cell`), so undo never reconstructs values
/// itself and dependents are recalculated by the same code as a normal edit.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub label: String,
    pub changes: Vec<CellChange>,
}

/// What an undo / redo did, so a UI can switch to the sheet and highlight the
/// cells that changed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryOutcome {
    pub label: String,
    pub sheet: usize,
    pub references: Vec<String>,
    pub result: EditResult,
}

/// Result of [`Workbook::find_and_replace`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReplaceOutcome {
    /// Cells whose displayed text equalled the search text (constants only).
    pub matched: usize,
    /// Cells actually changed.
    pub replaced: usize,
}

/// Partial formatting update. `None` leaves a property alone; for the
/// optional properties `Some(None)` clears it. In JSON a missing key means
/// "leave", `null` means "clear".
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatPatch {
    #[serde(default)]
    pub bold: Option<bool>,
    #[serde(default)]
    pub italic: Option<bool>,
    #[serde(default)]
    pub underline: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub font_size: Option<Option<u8>>,
    #[serde(default, deserialize_with = "double_option")]
    pub color: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub fill: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub align: Option<Option<Align>>,
    /// Reset every property first (then apply the others, if any).
    #[serde(default)]
    pub clear_all: bool,
}

fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

impl FormatPatch {
    pub fn apply(&self, format: &mut CellFormat) {
        if self.clear_all {
            *format = CellFormat::default();
        }
        if let Some(b) = self.bold {
            format.bold = b;
        }
        if let Some(i) = self.italic {
            format.italic = i;
        }
        if let Some(u) = self.underline {
            format.underline = u;
        }
        if let Some(size) = &self.font_size {
            format.font_size = *size;
        }
        if let Some(color) = &self.color {
            format.color = color.clone();
        }
        if let Some(fill) = &self.fill {
            format.fill = fill.clone();
        }
        if let Some(align) = &self.align {
            format.align = *align;
        }
    }
}

/// A `SORT(A1:B5;A;asc)` command (desktop formula-bar syntax).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortSpec {
    /// `(row1, col1, row2, col2)`, 0-based, inclusive.
    pub rect: (i32, i32, i32, i32),
    pub key: String,
    pub ascending: bool,
}

/// Default number of undo steps kept.
pub const HISTORY_LIMIT: usize = 200;

static CELL_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([A-Z]+)(\d+)$").unwrap());
static COMMAND_TARGET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([A-Z]+)(\d+)\s*=").unwrap());
static SORT_COMMAND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^SORT\(\s*([A-Z]+)(\d+)\s*:\s*([A-Z]+)(\d+)\s*;\s*([A-Z]+|\d+)\s*;\s*(asc|desc)\s*\)$",
    )
    .unwrap()
});

/// Parses the desktop `SORT(A1:B5;A;asc)` / `SORT(A1:D3;2;desc)` command.
pub fn parse_sort_command(text: &str) -> Option<SortSpec> {
    let caps = SORT_COMMAND.captures(text.trim())?;
    let row1: i32 = caps[2].parse().ok()?;
    let row2: i32 = caps[4].parse().ok()?;
    Some(SortSpec {
        rect: (
            row1 - 1,
            col_name_to_col_num(&caps[1]),
            row2 - 1,
            col_name_to_col_num(&caps[3]),
        ),
        key: caps[5].to_string(),
        ascending: &caps[6] == "asc",
    })
}

/// Parses `"B3"` into 0-based `(row, col)` = `(2, 1)`. Whitespace is trimmed,
/// letters must be upper-case (same rule as the command language).
pub fn parse_cell_ref(reference: &str) -> Result<(i32, i32), CoreError> {
    let trimmed = reference.trim();
    let caps = CELL_REF
        .captures(trimmed)
        .ok_or_else(|| CoreError::InvalidCellReference(reference.to_string()))?;
    let col = col_name_to_col_num(caps.get(1).unwrap().as_str());
    let row: i32 = caps
        .get(2)
        .unwrap()
        .as_str()
        .parse()
        .map_err(|_| CoreError::InvalidCellReference(reference.to_string()))?;
    if row < 1 {
        return Err(CoreError::InvalidCellReference(reference.to_string()));
    }
    Ok((row - 1, col))
}

/// Inverse of [`parse_cell_ref`]: `(2, 1)` -> `"B3"`.
pub fn cell_ref_name(row: i32, col: i32) -> String {
    format!("{}{}", col_num_to_col_name(col), row + 1)
}

fn normalize_rect((r1, c1, r2, c2): (i32, i32, i32, i32)) -> (i32, i32, i32, i32) {
    (r1.min(r2), c1.min(c2), r1.max(r2), c1.max(c2))
}

fn snapshot_cell(r: i16, c: i16, cell: &Cell, format: Option<&CellFormat>) -> CellSnapshot {
    CellSnapshot {
        row: r as i32,
        col: c as i32,
        reference: cell_ref_name(r as i32, c as i32),
        display: cell.display_value(),
        value: cell.value,
        is_error: cell.is_error,
        is_string: cell.string.is_some() && !cell.is_error,
        has_formula: cell.has_formula(),
        formula: get_cell_formula(r, c, cell),
        input: get_cell_input(r, c, cell),
        format: format.cloned(),
    }
}

/// Snapshot of a cell that may exist only in `formats` (no content).
fn snapshot_at(sheet: &Sheet, r: i16, c: i16) -> CellSnapshot {
    let format = sheet.formats.get(&(r, c));
    match sheet.data.get(&(r, c)) {
        Some(cell) => snapshot_cell(r, c, cell, format),
        None => {
            let blank = Cell::empty();
            let mut snap = snapshot_cell(r, c, &blank, format);
            snap.input = String::new();
            snap.display = String::new();
            snap
        }
    }
}

/// A collection of named sheets plus the (CLI-style) viewport used by the
/// navigation commands.
#[derive(Clone, Default)]
pub struct Workbook {
    sheets: Vec<NamedSheet>,
    view: Viewport,
    undo_stack: Vec<Transaction>,
    redo_stack: Vec<Transaction>,
}

impl Workbook {
    /// An empty workbook (no sheets).
    pub fn new() -> Self {
        Workbook::default()
    }

    /// A workbook with one sheet named `"Sheet 1"` of the given size.
    pub fn with_default_sheet(rows: i32, cols: i32) -> Result<Self, CoreError> {
        let mut wb = Workbook::new();
        wb.add_sheet("Sheet 1", rows, cols)?;
        Ok(wb)
    }

    /// Wraps existing sheets (e.g. the desktop's `Vec<Sheets>`).
    pub fn from_sheets(sheets: Vec<NamedSheet>) -> Self {
        Workbook {
            sheets,
            ..Workbook::default()
        }
    }

    pub fn sheets(&self) -> &[NamedSheet] {
        &self.sheets
    }

    pub fn into_sheets(self) -> Vec<NamedSheet> {
        self.sheets
    }

    pub fn view(&self) -> &Viewport {
        &self.view
    }

    // ------------------------------------------------------------ sheets

    pub fn sheet_count(&self) -> usize {
        self.sheets.len()
    }

    pub fn sheet_names(&self) -> Vec<String> {
        self.sheets.iter().map(|s| s.name.clone()).collect()
    }

    pub fn sheet(&self, index: usize) -> Result<&NamedSheet, CoreError> {
        self.sheets
            .get(index)
            .ok_or(CoreError::InvalidSheetIndex(index))
    }

    pub fn sheet_mut(&mut self, index: usize) -> Result<&mut NamedSheet, CoreError> {
        self.sheets
            .get_mut(index)
            .ok_or(CoreError::InvalidSheetIndex(index))
    }

    /// Appends a new empty sheet and returns its index.
    pub fn add_sheet(&mut self, name: &str, rows: i32, cols: i32) -> Result<usize, CoreError> {
        if rows <= 0 || cols <= 0 || rows > MAX_ROWS || cols > MAX_COLS {
            return Err(CoreError::InvalidDimensions { rows, cols });
        }
        self.sheets.push(NamedSheet {
            sheet: Sheet::new(rows, cols),
            name: name.to_string(),
        });
        Ok(self.sheets.len() - 1)
    }

    /// Appends a sheet built from CSV text and returns its index.
    pub fn add_sheet_from_csv(&mut self, name: &str, csv_text: &str) -> usize {
        self.sheets.push(NamedSheet {
            sheet: csv::sheet_from_csv(csv_text),
            name: name.to_string(),
        });
        self.sheets.len() - 1
    }

    /// Removes a sheet. Like the desktop, this clears the undo/redo history
    /// (later sheets shift index).
    pub fn remove_sheet(&mut self, index: usize) -> Result<NamedSheet, CoreError> {
        if index >= self.sheets.len() {
            return Err(CoreError::InvalidSheetIndex(index));
        }
        self.clear_history();
        Ok(self.sheets.remove(index))
    }

    pub fn rename_sheet(&mut self, index: usize, name: &str) -> Result<(), CoreError> {
        self.sheet_mut(index)?.name = name.to_string();
        Ok(())
    }

    // ------------------------------------------------------------ editing

    /// Runs one command of the command language (`A1=B1+5`, `A1="x"`,
    /// `A1=SUM(A1:B2)`, `scroll_toC3`, …) against sheet `index`.
    pub fn execute_command(
        &mut self,
        index: usize,
        command: &str,
    ) -> Result<EditResult, CoreError> {
        // Cell commands are undoable; navigation commands are not.
        let target: Vec<(i16, i16)> = COMMAND_TARGET
            .captures(command)
            .and_then(|c| parse_cell_ref(&format!("{}{}", &c[1], &c[2])).ok())
            .filter(|&(r, c)| {
                self.sheets
                    .get(index)
                    .map(|s| is_valid_cell(r, c, s.sheet.rows, s.sheet.cols))
                    .unwrap_or(false)
            })
            .map(|(r, c)| vec![(r as i16, c as i16)])
            .unwrap_or_default();
        let label = format!("command {}", command.trim());
        self.record(index, &label, target, |wb| {
            wb.execute_command_raw(index, command)
        })
    }

    fn execute_command_raw(
        &mut self,
        index: usize,
        command: &str,
    ) -> Result<EditResult, CoreError> {
        let view = &mut self.view;
        let named = self
            .sheets
            .get_mut(index)
            .ok_or(CoreError::InvalidSheetIndex(index))?;
        let rows = named.sheet.rows;
        let cols = named.sheet.cols;
        let mut status = String::new();
        let outcome = execute_command(command, view, &mut status, rows, cols, &mut named.sheet);
        Ok(EditResult::from_outcome(status, &outcome))
    }

    /// Sets the content of `reference` (`"A1"`) to `input`, where `input` is
    /// the right-hand side of a command: `5`, `B1+5`, `SUM(A1:B2)`, `"text"`.
    pub fn set_cell(
        &mut self,
        index: usize,
        reference: &str,
        input: &str,
    ) -> Result<EditResult, CoreError> {
        let (row, col) = self.checked_cell(index, reference)?;
        let command = format!("{}={}", cell_ref_name(row, col), input.trim());
        self.execute_command_raw(index, &command)
    }

    /// Convenience: stores `text` as a string cell (adds the quotes for you).
    pub fn set_cell_string(
        &mut self,
        index: usize,
        reference: &str,
        text: &str,
    ) -> Result<EditResult, CoreError> {
        self.set_cell(index, reference, &format!("\"{}\"", text))
    }

    /// Spreadsheet-style edit, the policy a grid editor wants:
    ///
    /// * empty / whitespace text -> [`clear_cell`](Self::clear_cell)
    /// * text starting with `=`  -> strict formula (`=A1+B1`, `=SUM(A1:B2)`);
    ///   problems are reported in the result
    /// * anything else           -> tried as formula / number first; if the
    ///   engine answers `Invalid cmd` it is stored as a string cell instead
    ///   (so typing `hello` just works, like other spreadsheets)
    pub fn edit_cell(
        &mut self,
        index: usize,
        reference: &str,
        text: &str,
    ) -> Result<EditResult, CoreError> {
        let (row, col) = self.checked_cell(index, reference)?;
        let label = format!("edit {}", cell_ref_name(row, col));
        self.record(index, &label, vec![(row as i16, col as i16)], |wb| {
            wb.edit_cell_raw(index, reference, text)
        })
    }

    fn edit_cell_raw(
        &mut self,
        index: usize,
        reference: &str,
        text: &str,
    ) -> Result<EditResult, CoreError> {
        let text = text.trim();
        if text.is_empty() {
            return self.clear_cell_raw(index, reference);
        }
        if let Some(formula) = text.strip_prefix('=') {
            return self.set_cell(index, reference, formula.trim());
        }
        let result = self.set_cell(index, reference, text)?;
        if result.status == "Invalid cmd" {
            let unquoted = text
                .strip_prefix('"')
                .and_then(|t| t.strip_suffix('"'))
                .unwrap_or(text);
            return self.set_cell_string(index, reference, unquoted);
        }
        Ok(result)
    }

    /// Resets a cell to "never written": drops its formula (dependents are
    /// recalculated and see 0) and removes it from storage if nothing
    /// references it, so it renders as blank again.
    pub fn clear_cell(&mut self, index: usize, reference: &str) -> Result<EditResult, CoreError> {
        let (row, col) = self.checked_cell(index, reference)?;
        let label = format!("clear {}", cell_ref_name(row, col));
        self.record(index, &label, vec![(row as i16, col as i16)], |wb| {
            wb.clear_cell_raw(index, reference)
        })
    }

    fn clear_cell_raw(&mut self, index: usize, reference: &str) -> Result<EditResult, CoreError> {
        let (row, col) = self.checked_cell(index, reference)?;
        if self.sheets[index]
            .sheet
            .data
            .get(&(row as i16, col as i16))
            .is_none()
        {
            return Ok(EditResult {
                ok: true,
                status: "ok".to_string(),
                sleep_seconds: 0,
            });
        }
        // `REF=0` goes through the engine: removes the old constraint,
        // recalculates dependents.
        let result = self.set_cell(index, reference, "0")?;
        let sheet = &mut self.sheets[index].sheet;
        let unreferenced = sheet
            .data
            .get(&(row as i16, col as i16))
            .map(|c| c.dependencies.is_empty())
            .unwrap_or(true);
        if unreferenced {
            sheet.data.remove(&(row as i16, col as i16));
        }
        Ok(result)
    }

    /// Clears every cell of the rectangle (one undo step).
    pub fn clear_range(
        &mut self,
        index: usize,
        rect: (i32, i32, i32, i32),
    ) -> Result<EditResult, CoreError> {
        let cells = self.rect_cells(index, rect)?;
        let label = format!("clear {} cell(s)", cells.len());
        self.record(index, &label, cells.clone(), |wb| {
            let mut last = EditResult::ok();
            for &(r, c) in &cells {
                if wb.sheets[index].sheet.data.contains_key(&(r, c)) {
                    last = wb.clear_cell_raw(index, &cell_ref_name(r as i32, c as i32))?;
                }
            }
            // Cells that were still referenced when cleared (by a formula
            // cleared later in the same pass) can now be dropped too.
            for &(r, c) in &cells {
                wb.prune_blank(index, r, c);
            }
            Ok(last)
        })
    }

    /// Drops a stored cell that is indistinguishable from a never-written one
    /// (plain 0, no text, no error, nothing references it).
    fn prune_blank(&mut self, index: usize, row: i16, col: i16) {
        let sheet = &mut self.sheets[index].sheet;
        let blank = sheet
            .data
            .get(&(row, col))
            .map(|c| {
                c.op_code == OpCode::NoConstraint
                    && c.value == 0
                    && c.string.is_none()
                    && !c.is_error
                    && c.dependencies.is_empty()
            })
            .unwrap_or(false);
        if blank {
            sheet.data.remove(&(row, col));
        }
    }

    /// Pastes a block of editor texts (e.g. tab-separated text from another
    /// application) with its top-left at `anchor`, through
    /// [`edit_cell`](Self::edit_cell) semantics, as one undo step. Cells
    /// outside the sheet are skipped.
    pub fn paste_text_block(
        &mut self,
        index: usize,
        anchor: (i32, i32),
        block: &[Vec<String>],
    ) -> Result<EditResult, CoreError> {
        let named = self.sheet(index)?;
        let (rows, cols) = (named.sheet.rows, named.sheet.cols);
        let mut cells = Vec::new();
        for (dr, line) in block.iter().enumerate() {
            for dc in 0..line.len() {
                let (r, c) = (anchor.0 + dr as i32, anchor.1 + dc as i32);
                if is_valid_cell(r, c, rows, cols) {
                    cells.push((r as i16, c as i16));
                }
            }
        }
        let label = format!("paste {} row(s)", block.len());
        self.record(index, &label, cells, |wb| {
            let mut worst = EditResult::ok();
            for (dr, line) in block.iter().enumerate() {
                for (dc, text) in line.iter().enumerate() {
                    let (r, c) = (anchor.0 + dr as i32, anchor.1 + dc as i32);
                    if !is_valid_cell(r, c, rows, cols) {
                        continue;
                    }
                    let result = wb.edit_cell_raw(index, &cell_ref_name(r, c), text)?;
                    if !result.ok {
                        worst = result;
                    }
                }
            }
            Ok(worst)
        })
    }

    /// Desktop "cut + paste": moves the cells of `from` so that the block's
    /// top-left lands on `to`. Formulas referencing a moved cell are rewritten
    /// to its new position (see [`move_cell`]). One undo step.
    pub fn move_cells(
        &mut self,
        index: usize,
        from: (i32, i32, i32, i32),
        to: (i32, i32),
    ) -> Result<EditResult, CoreError> {
        let (r1, c1, r2, c2) = normalize_rect(from);
        let (dr, dc) = (to.0 - r1, to.1 - c1);
        if dr == 0 && dc == 0 {
            return Ok(EditResult::ok());
        }
        let named = self.sheet(index)?;
        let (rows, cols) = (named.sheet.rows, named.sheet.cols);
        for &(r, c) in &[(r1, c1), (r2, c2)] {
            if !is_valid_cell(r, c, rows, cols) || !is_valid_cell(r + dr, c + dc, rows, cols) {
                return Err(CoreError::CellOutOfBounds {
                    reference: cell_ref_name(r + dr, c + dc),
                    rows,
                    cols,
                });
            }
        }
        // Visit cells so that a cell is never overwritten before it is moved
        // when source and target overlap.
        let row_iter: Vec<i32> = if dr > 0 {
            (r1..=r2).rev().collect()
        } else {
            (r1..=r2).collect()
        };
        let col_iter: Vec<i32> = if dc > 0 {
            (c1..=c2).rev().collect()
        } else {
            (c1..=c2).collect()
        };
        let mut order: Vec<(i32, i32)> = Vec::new();
        for &r in &row_iter {
            for &c in &col_iter {
                order.push((r, c));
            }
        }
        let mut touched = Vec::new();
        for &(r, c) in &order {
            for pos in [(r, c), (r + dr, c + dc)] {
                let info = CellInfo {
                    row: pos.0 as i16,
                    col: pos.1 as i16,
                };
                touched.push((info.row, info.col));
                for d in transitive_dependents(&self.sheets[index].sheet, info) {
                    touched.push((d.row, d.col));
                }
            }
        }
        let label = format!(
            "move {}:{} to {}",
            cell_ref_name(r1, c1),
            cell_ref_name(r2, c2),
            cell_ref_name(to.0, to.1)
        );
        self.record(index, &label, touched, |wb| {
            let sheet = &mut wb.sheets[index].sheet;
            for (r, c) in order {
                move_cell(
                    sheet,
                    CellInfo {
                        row: r as i16,
                        col: c as i16,
                    },
                    CellInfo {
                        row: (r + dr) as i16,
                        col: (c + dc) as i16,
                    },
                );
                // formatting travels with the cell
                let from = (r as i16, c as i16);
                let to = ((r + dr) as i16, (c + dc) as i16);
                match sheet.formats.remove(&from) {
                    Some(f) => {
                        sheet.formats.insert(to, f);
                    }
                    None => {
                        sheet.formats.remove(&to);
                    }
                }
            }
            Ok(EditResult::ok())
        })
    }

    /// Desktop "copy + paste": pastes the *current values* of `from` (not the
    /// formulas) with the block's top-left at `to`. One undo step.
    pub fn copy_values(
        &mut self,
        index: usize,
        from: (i32, i32, i32, i32),
        to: (i32, i32),
    ) -> Result<EditResult, CoreError> {
        let (r1, c1, r2, c2) = normalize_rect(from);
        let (dr, dc) = (to.0 - r1, to.1 - c1);
        if dr == 0 && dc == 0 {
            return Ok(EditResult::ok());
        }
        let named = self.sheet(index)?;
        let (rows, cols) = (named.sheet.rows, named.sheet.cols);
        if !is_valid_cell(r1, c1, rows, cols) || !is_valid_cell(to.0, to.1, rows, cols) {
            return Err(CoreError::CellOutOfBounds {
                reference: cell_ref_name(to.0, to.1),
                rows,
                cols,
            });
        }
        // Snapshot the sources first so overlapping targets cannot feed back.
        let mut pairs: Vec<((i16, i16), (i16, i16))> = Vec::new();
        for r in r1..=r2 {
            for c in c1..=c2 {
                if is_valid_cell(r + dr, c + dc, rows, cols) {
                    pairs.push(((r as i16, c as i16), ((r + dr) as i16, (c + dc) as i16)));
                }
            }
        }
        let sources: Vec<Option<Cell>> = pairs
            .iter()
            .map(|&(from, _)| self.sheets[index].sheet.data.get(&from).cloned())
            .collect();
        let source_formats: Vec<Option<CellFormat>> = pairs
            .iter()
            .map(|&(from, _)| self.sheets[index].sheet.formats.get(&from).cloned())
            .collect();
        let targets: Vec<(i16, i16)> = pairs.iter().map(|&(_, to)| to).collect();
        let label = format!("paste values at {}", cell_ref_name(to.0, to.1));
        self.record(index, &label, targets, |wb| {
            let sheet = &mut wb.sheets[index].sheet;
            for (k, &(from, to)) in pairs.iter().enumerate() {
                // Temporarily restore the pre-paste source so copy_cell_value
                // copies what the user saw even when ranges overlap.
                let live = sheet.data.get(&from).cloned();
                match &sources[k] {
                    Some(src) => {
                        sheet.data.insert(from, src.clone());
                    }
                    None => {
                        sheet.data.remove(&from);
                    }
                }
                copy_cell_value(
                    sheet,
                    CellInfo {
                        row: from.0,
                        col: from.1,
                    },
                    CellInfo {
                        row: to.0,
                        col: to.1,
                    },
                );
                if from != to {
                    match live {
                        Some(cell) => {
                            sheet.data.insert(from, cell);
                        }
                        None => {
                            sheet.data.remove(&from);
                        }
                    }
                }
                // A blank copied onto a cell nobody references renders blank.
                if sources[k].is_none() {
                    if let Some(cell) = sheet.data.get(&to) {
                        if cell.dependencies.is_empty() {
                            sheet.data.remove(&to);
                        }
                    }
                }
                match source_formats[k].clone() {
                    Some(f) => {
                        sheet.formats.insert(to, f);
                    }
                    None => {
                        sheet.formats.remove(&to);
                    }
                }
            }
            Ok(EditResult::ok())
        })
    }

    /// Cells whose displayed text or editor text contains `needle`
    /// (case-insensitive; `exact` requires the whole display text to match),
    /// row-major.
    pub fn find(
        &self,
        index: usize,
        needle: &str,
        exact: bool,
    ) -> Result<Vec<CellSnapshot>, CoreError> {
        let named = self.sheet(index)?;
        let lowered = needle.to_lowercase();
        let mut keys: Vec<&(i16, i16)> = named.sheet.data.keys().collect();
        keys.sort();
        Ok(keys
            .into_iter()
            .map(|&(r, c)| snapshot_at(&named.sheet, r, c))
            .filter(|snap| {
                if exact {
                    snap.display == needle
                } else {
                    !needle.is_empty()
                        && (snap.display.to_lowercase().contains(&lowered)
                            || snap.input.to_lowercase().contains(&lowered))
                }
            })
            .collect())
    }

    /// Desktop "Find and Replace": every *constant* cell (number or string)
    /// whose displayed text equals `find` exactly is rewritten to `replace`
    /// (as a number if the cell held a number, as text if it held text), and
    /// dependents are recalculated. Formula cells are never touched. One undo
    /// step.
    pub fn find_and_replace(
        &mut self,
        index: usize,
        find: &str,
        replace: &str,
    ) -> Result<ReplaceOutcome, CoreError> {
        let named = self.sheet(index)?;
        let mut matches: Vec<((i16, i16), bool)> = named
            .sheet
            .data
            .iter()
            .filter(|(_, cell)| {
                matches!(cell.op_code, OpCode::NoConstraint | OpCode::String)
                    && cell.display_value() == find
            })
            .map(|(&key, cell)| (key, !cell.is_error && cell.string.is_none()))
            .collect();
        matches.sort();
        let matched = matches.len();
        if matched == 0 {
            return Ok(ReplaceOutcome {
                matched: 0,
                replaced: 0,
            });
        }
        let mut replaced = 0;
        let keys: Vec<(i16, i16)> = matches.iter().map(|m| m.0).collect();
        let label = format!("replace \"{}\" with \"{}\"", find, replace);
        self.record(index, &label, keys, |wb| {
            for ((r, c), is_num) in matches {
                let reference = cell_ref_name(r as i32, c as i32);
                let result = if is_num {
                    wb.set_cell(index, &reference, replace)?
                } else {
                    wb.set_cell_string(index, &reference, replace)?
                };
                if result.ok {
                    replaced += 1;
                }
            }
            Ok(EditResult::ok())
        })?;
        Ok(ReplaceOutcome { matched, replaced })
    }

    /// Desktop "Sort": reorders the rows (column key such as `"B"`) or the
    /// columns (1-based row key such as `"3"`) of the rectangle by the key's
    /// numeric value, stable, ascending or descending. Same validation as the
    /// desktop (`wrong range`, `sorted column out of range`, `sorted column
    /// has string`, `range has constraints`) reported through the status.
    /// One undo step; formulas referencing the sorted cells are recalculated.
    pub fn sort_range(
        &mut self,
        index: usize,
        rect: (i32, i32, i32, i32),
        key: &str,
        ascending: bool,
    ) -> Result<EditResult, CoreError> {
        let (row1, col1, row2, col2) = rect;
        let named = self.sheet(index)?;
        let (rows, cols) = (named.sheet.rows, named.sheet.cols);
        let fail = |status: &str| {
            Ok(EditResult {
                ok: false,
                status: status.to_string(),
                sleep_seconds: 0,
            })
        };
        if col2 < col1 || row2 < row1 {
            return fail("wrong range");
        }
        if !is_valid_cell(row1, col1, rows, cols) || !is_valid_cell(row2, col2, rows, cols) {
            return fail("wrong range");
        }
        let is_column = !key.is_empty() && key.chars().all(|c| c.is_ascii_uppercase());
        let sheet = &named.sheet;
        let has_string = |r: i32, c: i32| {
            sheet
                .data
                .get(&(r as i16, c as i16))
                .map(|cell| cell.string.is_some())
                .unwrap_or(false)
        };
        if is_column {
            let sort_col = col_name_to_col_num(key);
            if sort_col > col2 || sort_col < col1 {
                return fail("sorted column out of range");
            }
            if (row1..=row2).any(|r| has_string(r, sort_col)) {
                return fail("sorted column has string");
            }
        } else {
            let sort_row = match key.parse::<i32>() {
                Ok(n) => n - 1,
                Err(_) => return fail("sorted row out of range"),
            };
            if sort_row > row2 || sort_row < row1 {
                return fail("sorted row out of range");
            }
            if (col1..=col2).any(|c| has_string(sort_row, c)) {
                return fail("sorted row has string");
            }
        }
        for r in row1..=row2 {
            for c in col1..=col2 {
                if sheet
                    .data
                    .get(&(r as i16, c as i16))
                    .map(|cell| cell.has_formula())
                    .unwrap_or(false)
                {
                    return fail("range has constraints");
                }
            }
        }

        let order = sort_order(
            sheet,
            col1,
            row1,
            col2,
            row2,
            key,
            is_column,
            if ascending { "asc" } else { "desc" },
        );
        // Editor texts of every line before the sort; applying them through
        // the engine keeps dependents of the sorted cells up to date.
        let input_at = |r: i32, c: i32| -> String {
            sheet
                .data
                .get(&(r as i16, c as i16))
                .map(|cell| get_cell_input(r as i16, c as i16, cell))
                .unwrap_or_default()
        };
        let format_at = |r: i32, c: i32| -> Option<CellFormat> {
            sheet.formats.get(&(r as i16, c as i16)).cloned()
        };
        let mut assignments: Vec<((i32, i32), String, Option<CellFormat>)> = Vec::new();
        if is_column {
            for (k, &src) in order.iter().enumerate() {
                for c in col1..=col2 {
                    let from = (row1 + src as i32, c);
                    assignments.push((
                        (row1 + k as i32, c),
                        input_at(from.0, from.1),
                        format_at(from.0, from.1),
                    ));
                }
            }
        } else {
            for (k, &src) in order.iter().enumerate() {
                for r in row1..=row2 {
                    let from = (r, col1 + src as i32);
                    assignments.push((
                        (r, col1 + k as i32),
                        input_at(from.0, from.1),
                        format_at(from.0, from.1),
                    ));
                }
            }
        }
        let cells = self.rect_cells(index, rect)?;
        let label = format!(
            "sort {}:{} by {}",
            cell_ref_name(row1, col1),
            cell_ref_name(row2, col2),
            key
        );
        self.record(index, &label, cells, |wb| {
            for ((r, c), text, format) in assignments {
                let reference = cell_ref_name(r, c);
                wb.set_format_raw(index, r as i16, c as i16, format);
                if wb.cell_input(index, &reference)? == text {
                    continue;
                }
                if text.is_empty() {
                    wb.clear_cell_raw(index, &reference)?;
                } else {
                    wb.set_cell(index, &reference, &text)?;
                }
            }
            Ok(EditResult::ok())
        })
    }

    /// Runs a desktop-style `SORT(A1:B5;A;asc)` command; `Ok(None)` when the
    /// text is not a sort command.
    pub fn sort_command(
        &mut self,
        index: usize,
        text: &str,
    ) -> Result<Option<EditResult>, CoreError> {
        match parse_sort_command(text) {
            Some(spec) => Ok(Some(self.sort_range(
                index,
                spec.rect,
                &spec.key,
                spec.ascending,
            )?)),
            None => Ok(None),
        }
    }

    // ------------------------------------------------------------ formatting

    /// Formatting of a cell (default when none was set).
    pub fn cell_format(&self, index: usize, reference: &str) -> Result<CellFormat, CoreError> {
        let (row, col) = self.checked_cell(index, reference)?;
        Ok(self
            .format_at(index, row as i16, col as i16)
            .unwrap_or_default())
    }

    /// Applies a formatting patch to every cell of the rectangle (one undo
    /// step). Formatting never changes values or formulas.
    pub fn set_format(
        &mut self,
        index: usize,
        rect: (i32, i32, i32, i32),
        patch: &FormatPatch,
    ) -> Result<EditResult, CoreError> {
        let cells = self.rect_cells(index, rect)?;
        let label = format!("format {} cell(s)", cells.len());
        self.record(index, &label, cells.clone(), |wb| {
            for (r, c) in cells {
                let mut format = wb.format_at(index, r, c).unwrap_or_default();
                patch.apply(&mut format);
                wb.set_format_raw(index, r, c, Some(format));
            }
            Ok(EditResult::ok())
        })
    }

    // ------------------------------------------------------------ charts

    pub fn charts(&self, index: usize) -> Result<Vec<ChartSpec>, CoreError> {
        Ok(self.sheet(index)?.sheet.charts.clone())
    }

    /// Stores a chart definition on the sheet and returns its id.
    pub fn add_chart(&mut self, index: usize, mut spec: ChartSpec) -> Result<u32, CoreError> {
        let sheet = &mut self.sheet_mut(index)?.sheet;
        let next = sheet.charts.iter().map(|c| c.id).max().unwrap_or(0) + 1;
        spec.id = next;
        spec.range = spec.range.normalized();
        sheet.charts.push(spec);
        Ok(next)
    }

    pub fn update_chart(&mut self, index: usize, mut spec: ChartSpec) -> Result<(), CoreError> {
        let sheet = &mut self.sheet_mut(index)?.sheet;
        spec.range = spec.range.normalized();
        match sheet.charts.iter_mut().find(|c| c.id == spec.id) {
            Some(existing) => {
                *existing = spec;
                Ok(())
            }
            None => Err(CoreError::InvalidChart(spec.id)),
        }
    }

    pub fn remove_chart(&mut self, index: usize, id: u32) -> Result<(), CoreError> {
        let sheet = &mut self.sheet_mut(index)?.sheet;
        let before = sheet.charts.len();
        sheet.charts.retain(|c| c.id != id);
        if sheet.charts.len() == before {
            return Err(CoreError::InvalidChart(id));
        }
        Ok(())
    }

    /// Live data for a chart definition (stored or not), read from the cells.
    pub fn chart_data(&self, index: usize, spec: &ChartSpec) -> Result<ChartData, CoreError> {
        Ok(chart_data(&self.sheet(index)?.sheet, spec))
    }

    // ------------------------------------------------------------ history

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    pub fn undo_labels(&self) -> Vec<String> {
        self.undo_stack.iter().map(|t| t.label.clone()).collect()
    }

    pub fn clear_history(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Reverts the last undoable operation; `Ok(None)` when there is nothing
    /// to undo.
    pub fn undo(&mut self) -> Result<Option<HistoryOutcome>, CoreError> {
        let Some(tx) = self.undo_stack.pop() else {
            return Ok(None);
        };
        let reversed: Vec<CellChange> = tx.changes.iter().rev().cloned().collect();
        let outcome = self.replay(
            &tx.label,
            &reversed,
            |c| &c.before,
            |c| c.before_format.clone(),
        )?;
        self.redo_stack.push(tx);
        Ok(Some(outcome))
    }

    pub fn redo(&mut self) -> Result<Option<HistoryOutcome>, CoreError> {
        let Some(tx) = self.redo_stack.pop() else {
            return Ok(None);
        };
        let outcome = self.replay(
            &tx.label,
            &tx.changes,
            |c| &c.after,
            |c| c.after_format.clone(),
        )?;
        self.undo_stack.push(tx);
        Ok(Some(outcome))
    }

    fn replay(
        &mut self,
        label: &str,
        changes: &[CellChange],
        pick: impl Fn(&CellChange) -> &String,
        pick_format: impl Fn(&CellChange) -> Option<CellFormat>,
    ) -> Result<HistoryOutcome, CoreError> {
        let mut worst = EditResult::ok();
        let mut references = Vec::new();
        let mut sheet = 0;
        let mut cleared = Vec::new();
        for change in changes {
            sheet = change.sheet;
            references.push(change.reference.clone());
            if change.before != change.after {
                let text = pick(change);
                let result = if text.is_empty() {
                    cleared.push((change.sheet, change.reference.clone()));
                    self.clear_cell_raw(change.sheet, &change.reference)?
                } else {
                    self.set_cell(change.sheet, &change.reference, text)?
                };
                if !result.ok {
                    worst = result;
                }
            }
            if change.before_format != change.after_format {
                let (r, c) = self.checked_cell(change.sheet, &change.reference)?;
                let format = pick_format(change);
                self.set_format_raw(change.sheet, r as i16, c as i16, format);
            }
        }
        for (index, reference) in cleared {
            if let Ok((r, c)) = parse_cell_ref(&reference) {
                self.prune_blank(index, r as i16, c as i16);
            }
        }
        Ok(HistoryOutcome {
            label: label.to_string(),
            sheet,
            references,
            result: worst,
        })
    }

    /// Runs `op` and records the editor-text changes of `touched` cells as
    /// one transaction (if anything changed).
    fn record<F>(
        &mut self,
        index: usize,
        label: &str,
        mut touched: Vec<(i16, i16)>,
        op: F,
    ) -> Result<EditResult, CoreError>
    where
        F: FnOnce(&mut Self) -> Result<EditResult, CoreError>,
    {
        self.sheet(index)?;
        touched.sort();
        touched.dedup();
        let before: Vec<(String, Option<CellFormat>)> = touched
            .iter()
            .map(|&(r, c)| (self.input_at(index, r, c), self.format_at(index, r, c)))
            .collect();
        let result = op(self)?;
        let mut changes = Vec::new();
        for (k, &(r, c)) in touched.iter().enumerate() {
            let after = self.input_at(index, r, c);
            let after_format = self.format_at(index, r, c);
            if after != before[k].0 || after_format != before[k].1 {
                changes.push(CellChange {
                    sheet: index,
                    reference: cell_ref_name(r as i32, c as i32),
                    before: before[k].0.clone(),
                    after,
                    before_format: before[k].1.clone(),
                    after_format,
                });
            }
        }
        if !changes.is_empty() {
            self.undo_stack.push(Transaction {
                label: label.to_string(),
                changes,
            });
            if self.undo_stack.len() > HISTORY_LIMIT {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
        Ok(result)
    }

    fn format_at(&self, index: usize, row: i16, col: i16) -> Option<CellFormat> {
        self.sheets[index].sheet.formats.get(&(row, col)).cloned()
    }

    fn set_format_raw(&mut self, index: usize, row: i16, col: i16, format: Option<CellFormat>) {
        let formats = &mut self.sheets[index].sheet.formats;
        match format {
            Some(f) if !f.is_default() => {
                formats.insert((row, col), f);
            }
            _ => {
                formats.remove(&(row, col));
            }
        }
    }

    fn input_at(&self, index: usize, row: i16, col: i16) -> String {
        self.sheets[index]
            .sheet
            .data
            .get(&(row, col))
            .map(|c| get_cell_input(row, col, c))
            .unwrap_or_default()
    }

    fn rect_cells(
        &self,
        index: usize,
        rect: (i32, i32, i32, i32),
    ) -> Result<Vec<(i16, i16)>, CoreError> {
        let named = self.sheet(index)?;
        let (r1, c1, r2, c2) = normalize_rect(rect);
        let (rows, cols) = (named.sheet.rows, named.sheet.cols);
        if !is_valid_cell(r1, c1, rows, cols) || !is_valid_cell(r2, c2, rows, cols) {
            return Err(CoreError::CellOutOfBounds {
                reference: format!("{}:{}", cell_ref_name(r1, c1), cell_ref_name(r2, c2)),
                rows,
                cols,
            });
        }
        let mut cells = Vec::new();
        for r in r1..=r2 {
            for c in c1..=c2 {
                cells.push((r as i16, c as i16));
            }
        }
        Ok(cells)
    }

    // ------------------------------------------------------------ reading

    /// The stored cell, or `None` if it was never written (implicitly `0`).
    pub fn cell(&self, index: usize, reference: &str) -> Result<Option<&Cell>, CoreError> {
        let (row, col) = self.checked_cell(index, reference)?;
        Ok(self.sheets[index].sheet.data.get(&(row as i16, col as i16)))
    }

    /// Text to show in the grid: `Err`, the string, or the number (`"0"` for
    /// untouched cells).
    pub fn display_value(&self, index: usize, reference: &str) -> Result<String, CoreError> {
        Ok(self
            .cell(index, reference)?
            .map(|c| c.display_value())
            .unwrap_or_else(|| "0".to_string()))
    }

    /// Numeric value (0 for untouched / string / error cells).
    pub fn cell_value(&self, index: usize, reference: &str) -> Result<i32, CoreError> {
        Ok(self.cell(index, reference)?.map(|c| c.value).unwrap_or(0))
    }

    pub fn cell_is_error(&self, index: usize, reference: &str) -> Result<bool, CoreError> {
        Ok(self
            .cell(index, reference)?
            .map(|c| c.is_error)
            .unwrap_or(false))
    }

    /// Full command that recreates the cell, e.g. `"B2=A1+5"` (`"B2=0"` for
    /// untouched cells).
    pub fn formula(&self, index: usize, reference: &str) -> Result<String, CoreError> {
        let (row, col) = self.checked_cell(index, reference)?;
        let cell = self.sheets[index].sheet.data.get(&(row as i16, col as i16));
        Ok(match cell {
            Some(c) => get_cell_formula(row as i16, col as i16, c),
            None => format!("{}=0", cell_ref_name(row, col)),
        })
    }

    /// Editor text for a cell (`A1+5`, `"hello"`, `5`; `""` for untouched).
    pub fn cell_input(&self, index: usize, reference: &str) -> Result<String, CoreError> {
        let (row, col) = self.checked_cell(index, reference)?;
        Ok(self.sheets[index]
            .sheet
            .data
            .get(&(row as i16, col as i16))
            .map(|c| get_cell_input(row as i16, col as i16, c))
            .unwrap_or_default())
    }

    /// Every non-empty cell of the sheet, row-major.
    pub fn sheet_snapshot(&self, index: usize) -> Result<SheetSnapshot, CoreError> {
        let named = self.sheet(index)?;
        let mut keys: Vec<(i16, i16)> = named
            .sheet
            .data
            .keys()
            .chain(named.sheet.formats.keys())
            .copied()
            .collect();
        keys.sort();
        keys.dedup();
        let cells = keys
            .into_iter()
            .map(|(r, c)| snapshot_at(&named.sheet, r, c))
            .collect();
        Ok(SheetSnapshot {
            name: named.name.clone(),
            rows: named.sheet.rows,
            cols: named.sheet.cols,
            cells,
        })
    }

    /// Non-empty cells inside the rectangle `(row1,col1)..=(row2,col2)`
    /// (0-based, inclusive, clipped to the sheet), row-major. This is what a
    /// virtualized grid asks for on every scroll, so it only touches the
    /// visible cells instead of the whole sheet.
    pub fn range_snapshot(
        &self,
        index: usize,
        row1: i32,
        col1: i32,
        row2: i32,
        col2: i32,
    ) -> Result<Vec<CellSnapshot>, CoreError> {
        let named = self.sheet(index)?;
        let sheet = &named.sheet;
        let r1 = row1.max(0);
        let c1 = col1.max(0);
        let r2 = row2.min(sheet.rows - 1);
        let c2 = col2.min(sheet.cols - 1);
        let mut cells = Vec::new();
        if r1 > r2 || c1 > c2 {
            return Ok(cells);
        }
        let area = (r2 - r1 + 1) as usize * (c2 - c1 + 1) as usize;
        let inside = |(r, c): &(i16, i16)| {
            (*r as i32) >= r1 && (*r as i32) <= r2 && (*c as i32) >= c1 && (*c as i32) <= c2
        };
        if area <= sheet.data.len() + sheet.formats.len() {
            for r in r1..=r2 {
                for c in c1..=c2 {
                    let key = (r as i16, c as i16);
                    if sheet.data.contains_key(&key) || sheet.formats.contains_key(&key) {
                        cells.push(snapshot_at(sheet, key.0, key.1));
                    }
                }
            }
        } else {
            // sparse sheet: iterate the stored cells instead of the rectangle
            let mut keys: Vec<(i16, i16)> = sheet
                .data
                .keys()
                .chain(sheet.formats.keys())
                .filter(|k| inside(k))
                .copied()
                .collect();
            keys.sort();
            keys.dedup();
            for (r, c) in keys {
                cells.push(snapshot_at(sheet, r, c));
            }
        }
        Ok(cells)
    }

    // ------------------------------------------------------------ persistence

    /// JSON in the same format as the desktop `.290` files
    /// (`Vec<NamedSheet>`).
    pub fn to_json(&self) -> Result<String, CoreError> {
        serde_json::to_string(&self.sheets).map_err(|e| CoreError::Json(e.to_string()))
    }

    pub fn to_json_pretty(&self) -> Result<String, CoreError> {
        serde_json::to_string_pretty(&self.sheets).map_err(|e| CoreError::Json(e.to_string()))
    }

    pub fn from_json(json: &str) -> Result<Self, CoreError> {
        let sheets: Vec<NamedSheet> =
            serde_json::from_str(json).map_err(|e| CoreError::Json(e.to_string()))?;
        Ok(Workbook::from_sheets(sheets))
    }

    pub fn sheet_to_csv(&self, index: usize) -> Result<String, CoreError> {
        Ok(csv::sheet_to_csv(&self.sheet(index)?.sheet))
    }

    // ------------------------------------------------------------ helpers

    fn checked_cell(&self, index: usize, reference: &str) -> Result<(i32, i32), CoreError> {
        let named = self.sheet(index)?;
        let (row, col) = parse_cell_ref(reference)?;
        if !is_valid_cell(row, col, named.sheet.rows, named.sheet.cols) {
            return Err(CoreError::CellOutOfBounds {
                reference: reference.trim().to_string(),
                rows: named.sheet.rows,
                cols: named.sheet.cols,
            });
        }
        Ok((row, col))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wb() -> Workbook {
        Workbook::with_default_sheet(10, 10).unwrap()
    }

    #[test]
    fn cell_reference_parsing() {
        assert_eq!(parse_cell_ref("A1"), Ok((0, 0)));
        assert_eq!(parse_cell_ref(" B3 "), Ok((2, 1)));
        assert_eq!(parse_cell_ref("AA10"), Ok((9, 26)));
        assert!(parse_cell_ref("a1").is_err());
        assert!(parse_cell_ref("A0").is_err());
        assert!(parse_cell_ref("1A").is_err());
        assert!(parse_cell_ref("").is_err());
        assert_eq!(cell_ref_name(2, 1), "B3");
        assert_eq!(cell_ref_name(9, 26), "AA10");
    }

    #[test]
    fn set_and_read_cells() {
        let mut wb = wb();
        assert!(wb.set_cell(0, "A1", "10").unwrap().ok);
        assert!(wb.set_cell(0, "B1", "A1*3").unwrap().ok);
        assert!(wb.set_cell(0, "C1", "SUM(A1:B1)").unwrap().ok);
        assert_eq!(wb.display_value(0, "B1").unwrap(), "30");
        assert_eq!(wb.display_value(0, "C1").unwrap(), "40");
        assert_eq!(wb.formula(0, "B1").unwrap(), "B1=A1*3");
        assert_eq!(wb.formula(0, "C1").unwrap(), "C1=SUM(A1:B1)");
        assert_eq!(wb.display_value(0, "J10").unwrap(), "0");
        assert_eq!(wb.formula(0, "J10").unwrap(), "J10=0");

        // recalculation cascades
        wb.set_cell(0, "A1", "1").unwrap();
        assert_eq!(wb.cell_value(0, "B1").unwrap(), 3);
        assert_eq!(wb.cell_value(0, "C1").unwrap(), 4);
    }

    #[test]
    fn strings_and_errors() {
        let mut wb = wb();
        wb.set_cell_string(0, "A1", "hello").unwrap();
        assert_eq!(wb.display_value(0, "A1").unwrap(), "hello");
        assert_eq!(wb.formula(0, "A1").unwrap(), "A1=hello");

        wb.set_cell(0, "B1", "0").unwrap();
        wb.set_cell(0, "C1", "5/B1").unwrap();
        assert!(wb.cell_is_error(0, "C1").unwrap());
        assert_eq!(wb.display_value(0, "C1").unwrap(), "Err");
        wb.set_cell(0, "B1", "5").unwrap();
        assert_eq!(wb.display_value(0, "C1").unwrap(), "1");
    }

    #[test]
    fn engine_status_is_reported_not_thrown() {
        let mut wb = wb();
        wb.set_cell(0, "A1", "B1").unwrap();
        let r = wb.set_cell(0, "B1", "A1").unwrap();
        assert!(!r.ok);
        assert_eq!(r.status, "circular error");

        let r = wb.set_cell(0, "A1", "garbage").unwrap();
        assert_eq!(r.status, "Invalid cmd");

        let r = wb.set_cell(0, "A1", "SLEEP(2)").unwrap();
        assert!(r.ok);
        assert_eq!(r.sleep_seconds, 2);
        assert_eq!(wb.cell_value(0, "A1").unwrap(), 2);
    }

    #[test]
    fn api_errors() {
        let mut wb = wb();
        assert_eq!(
            wb.set_cell(3, "A1", "1").unwrap_err(),
            CoreError::InvalidSheetIndex(3)
        );
        assert!(matches!(
            wb.set_cell(0, "Z99", "1").unwrap_err(),
            CoreError::CellOutOfBounds { .. }
        ));
        assert!(matches!(
            wb.display_value(0, "nope").unwrap_err(),
            CoreError::InvalidCellReference(_)
        ));
        assert!(matches!(
            wb.add_sheet("x", 0, 5).unwrap_err(),
            CoreError::InvalidDimensions { .. }
        ));
        assert!(wb.add_sheet("x", MAX_ROWS + 1, 5).is_err());
    }

    #[test]
    fn sheet_operations() {
        let mut wb = wb();
        assert_eq!(wb.add_sheet("Data", 5, 5).unwrap(), 1);
        assert_eq!(wb.sheet_names(), vec!["Sheet 1", "Data"]);
        wb.rename_sheet(1, "Renamed").unwrap();
        assert_eq!(wb.sheet(1).unwrap().name, "Renamed");
        wb.set_cell(1, "A1", "42").unwrap();
        assert_eq!(wb.display_value(1, "A1").unwrap(), "42");
        assert_eq!(wb.display_value(0, "A1").unwrap(), "0");
        let removed = wb.remove_sheet(0).unwrap();
        assert_eq!(removed.name, "Sheet 1");
        assert_eq!(wb.sheet_count(), 1);
        assert_eq!(wb.display_value(0, "A1").unwrap(), "42");
        assert!(wb.remove_sheet(5).is_err());
    }

    #[test]
    fn snapshot_lists_non_empty_cells_in_order() {
        let mut wb = wb();
        wb.set_cell(0, "B2", "2").unwrap();
        wb.set_cell(0, "A1", "1").unwrap();
        wb.set_cell(0, "C1", "A1+B2").unwrap();
        let snap = wb.sheet_snapshot(0).unwrap();
        assert_eq!(snap.rows, 10);
        let refs: Vec<&str> = snap.cells.iter().map(|c| c.reference.as_str()).collect();
        assert_eq!(refs, vec!["A1", "C1", "B2"]);
        let c1 = &snap.cells[1];
        assert_eq!(c1.display, "3");
        assert!(c1.has_formula);
        assert_eq!(c1.formula, "C1=A1+B2");
    }

    #[test]
    fn json_round_trip_keeps_formulas_live() {
        let mut wb = wb();
        wb.add_sheet("Second", 4, 4).unwrap();
        wb.set_cell(0, "A1", "5").unwrap();
        wb.set_cell(0, "B1", "A1+1").unwrap();
        wb.set_cell(1, "A1", "SUM(A2:B3)").unwrap();
        wb.set_cell_string(1, "B1", "txt").unwrap();

        let json = wb.to_json().unwrap();
        let mut back = Workbook::from_json(&json).unwrap();
        assert_eq!(back.sheet_names(), vec!["Sheet 1", "Second"]);
        assert_eq!(back.display_value(0, "B1").unwrap(), "6");
        assert_eq!(back.display_value(1, "B1").unwrap(), "txt");
        // dependencies survived: editing A1 still updates B1
        back.set_cell(0, "A1", "10").unwrap();
        assert_eq!(back.display_value(0, "B1").unwrap(), "11");
        back.set_cell(1, "A2", "7").unwrap();
        assert_eq!(back.display_value(1, "A1").unwrap(), "7");

        assert!(Workbook::from_json("not json").is_err());
    }

    #[test]
    fn csv_import_export() {
        let mut wb = Workbook::new();
        let idx = wb.add_sheet_from_csv("csv", "1,2\n3,x\n");
        assert_eq!(idx, 0);
        assert_eq!(wb.display_value(0, "B2").unwrap(), "x");
        wb.set_cell(0, "C1", "SUM(A1:B2)").unwrap_err(); // C1 is outside the 2x2 sheet
        wb.set_cell(0, "A2", "A1+B1").unwrap();
        assert_eq!(wb.sheet_to_csv(0).unwrap(), "1,2\n3,x\n");
    }

    #[test]
    fn cell_input_round_trips_through_the_parser() {
        let mut wb = wb();
        wb.set_cell(0, "A1", "5").unwrap();
        wb.set_cell(0, "B1", "A1+5").unwrap();
        wb.set_cell(0, "C1", "7-A1").unwrap();
        wb.set_cell(0, "D1", "SUM(A1:C1)").unwrap();
        wb.set_cell_string(0, "E1", "hi there").unwrap();
        wb.set_cell(0, "F1", "SLEEP(A1)").unwrap();
        for (r, expected) in [
            ("A1", "5"),
            ("B1", "A1+5"),
            ("C1", "7-A1"),
            ("D1", "SUM(A1:C1)"),
            ("E1", "\"hi there\""),
            ("F1", "SLEEP(A1)"),
        ] {
            let input = wb.cell_input(0, r).unwrap();
            assert_eq!(input, expected);
            // re-applying the input must be accepted and change nothing
            let before = wb.display_value(0, r).unwrap();
            assert!(wb.set_cell(0, r, &input).unwrap().ok);
            assert_eq!(wb.display_value(0, r).unwrap(), before);
        }
        assert_eq!(wb.cell_input(0, "J10").unwrap(), "");
        let snap = wb.range_snapshot(0, 0, 0, 0, 1).unwrap();
        assert_eq!(snap[1].input, "A1+5");
    }

    #[test]
    fn edit_cell_policy() {
        let mut wb = wb();
        // plain text becomes a string cell
        assert!(wb.edit_cell(0, "A1", "hello world").unwrap().ok);
        assert_eq!(wb.display_value(0, "A1").unwrap(), "hello world");
        assert_eq!(wb.cell_input(0, "A1").unwrap(), "\"hello world\"");
        // quoted text keeps working and is not double quoted
        wb.edit_cell(0, "A2", "\"quoted\"").unwrap();
        assert_eq!(wb.display_value(0, "A2").unwrap(), "quoted");
        // numbers / formulas without '='
        wb.edit_cell(0, "B1", "4").unwrap();
        wb.edit_cell(0, "C1", "B1*2").unwrap();
        assert_eq!(wb.cell_value(0, "C1").unwrap(), 8);
        // Excel style leading '='
        wb.edit_cell(0, "D1", " = SUM(B1:C1) ").unwrap();
        assert_eq!(wb.cell_value(0, "D1").unwrap(), 12);
        // a broken formula with '=' is reported, not turned into text
        let r = wb.edit_cell(0, "E1", "=SUM(B1:C1").unwrap();
        assert_eq!(r.status, "Invalid cmd");
        assert_eq!(wb.cell_input(0, "E1").unwrap(), "");
        // circular references are reported
        wb.edit_cell(0, "F1", "=G1").unwrap();
        let r = wb.edit_cell(0, "G1", "=F1").unwrap();
        assert_eq!(r.status, "circular error");
        // whitespace clears
        assert!(wb.edit_cell(0, "A1", "   ").unwrap().ok);
        assert_eq!(wb.cell_input(0, "A1").unwrap(), "");
    }

    #[test]
    fn clear_cell_recalculates_dependents_and_frees_storage() {
        let mut wb = wb();
        wb.set_cell(0, "A1", "5").unwrap();
        wb.set_cell(0, "B1", "A1*2").unwrap();
        wb.set_cell(0, "C1", "SUM(A1:B1)").unwrap();
        assert_eq!(wb.cell_value(0, "C1").unwrap(), 15);

        // A1 is referenced by B1 -> stays stored (as 0) but dependents update
        assert!(wb.clear_cell(0, "A1").unwrap().ok);
        assert_eq!(wb.cell_value(0, "B1").unwrap(), 0);
        assert_eq!(wb.cell_value(0, "C1").unwrap(), 0);
        assert!(wb.cell(0, "A1").unwrap().is_some());
        assert_eq!(wb.cell_input(0, "A1").unwrap(), "0");

        // B1 has a formula but nothing references it directly -> removed
        wb.clear_cell(0, "B1").unwrap();
        assert!(wb.cell(0, "B1").unwrap().is_none());
        assert_eq!(wb.cell_input(0, "B1").unwrap(), "");
        assert!(wb.clear_cell(0, "B1").unwrap().ok); // idempotent

        // the range formula still works after its inputs vanished
        wb.set_cell(0, "A1", "3").unwrap();
        assert_eq!(wb.cell_value(0, "C1").unwrap(), 3);
        wb.clear_cell(0, "C1").unwrap();
        wb.set_cell(0, "A1", "4").unwrap(); // no dangling range dependency
        assert_eq!(wb.range_snapshot(0, 0, 0, 0, 9).unwrap().len(), 1);
    }

    #[test]
    fn range_snapshot_is_clipped_and_ordered() {
        let mut wb = wb();
        wb.set_cell(0, "A1", "1").unwrap();
        wb.set_cell(0, "C2", "2").unwrap();
        wb.set_cell(0, "B2", "3").unwrap();
        wb.set_cell(0, "J10", "4").unwrap();
        let refs = |cells: Vec<CellSnapshot>| -> Vec<String> {
            cells.into_iter().map(|c| c.reference).collect()
        };
        assert_eq!(
            refs(wb.range_snapshot(0, 0, 0, 1, 2).unwrap()),
            vec!["A1", "B2", "C2"]
        );
        assert_eq!(
            refs(wb.range_snapshot(0, 1, 1, 50, 50).unwrap()),
            vec!["B2", "C2", "J10"]
        );
        assert!(wb.range_snapshot(0, 5, 5, 2, 2).unwrap().is_empty());
        assert!(wb.range_snapshot(0, -5, -5, -1, -1).unwrap().is_empty());
        assert!(wb.range_snapshot(2, 0, 0, 1, 1).is_err());
    }

    // ------------------------------------------------------------ history

    #[test]
    fn undo_redo_single_edits() {
        let mut wb = wb();
        assert!(!wb.can_undo());
        wb.edit_cell(0, "A1", "1").unwrap();
        wb.edit_cell(0, "A1", "2").unwrap();
        wb.edit_cell(0, "B1", "=A1*10").unwrap();
        assert_eq!(wb.undo_labels(), vec!["edit A1", "edit A1", "edit B1"]);

        let out = wb.undo().unwrap().unwrap();
        assert_eq!(out.references, vec!["B1"]);
        assert_eq!(wb.cell_input(0, "B1").unwrap(), "");
        wb.undo().unwrap();
        assert_eq!(wb.display_value(0, "A1").unwrap(), "1");
        wb.undo().unwrap();
        assert_eq!(wb.cell_input(0, "A1").unwrap(), "");
        assert!(wb.undo().unwrap().is_none());

        wb.redo().unwrap();
        wb.redo().unwrap();
        wb.redo().unwrap();
        assert_eq!(wb.display_value(0, "B1").unwrap(), "20");
        assert!(wb.redo().unwrap().is_none());

        // a new edit discards the redo branch; a no-op edit records nothing
        wb.undo().unwrap();
        wb.edit_cell(0, "C1", "5").unwrap();
        assert!(!wb.can_redo());
        let n = wb.undo_labels().len();
        wb.edit_cell(0, "C1", "5").unwrap();
        assert_eq!(wb.undo_labels().len(), n);
        let r = wb.edit_cell(0, "C1", "=D1").unwrap();
        assert!(r.ok);
        let r = wb.edit_cell(0, "D1", "=C1").unwrap(); // circular -> rejected
        assert!(!r.ok);
        assert_eq!(wb.undo_labels().len(), n + 1);
    }

    #[test]
    fn undo_spans_sheets_and_clears_on_sheet_removal() {
        let mut wb = wb();
        wb.add_sheet("Two", 5, 5).unwrap();
        wb.edit_cell(0, "A1", "1").unwrap();
        wb.edit_cell(1, "A1", "2").unwrap();
        let out = wb.undo().unwrap().unwrap();
        assert_eq!(out.sheet, 1);
        assert_eq!(wb.cell_input(1, "A1").unwrap(), "");
        assert_eq!(wb.display_value(0, "A1").unwrap(), "1");
        wb.remove_sheet(1).unwrap();
        assert!(!wb.can_undo() && !wb.can_redo());
    }

    #[test]
    fn clear_range_and_paste_block_are_single_undo_steps() {
        let mut wb = wb();
        let block = vec![
            vec!["1".to_string(), "2".to_string()],
            vec!["=A1+B1".to_string(), "hello".to_string()],
        ];
        wb.paste_text_block(0, (0, 0), &block).unwrap();
        assert_eq!(wb.display_value(0, "A2").unwrap(), "3");
        assert_eq!(wb.display_value(0, "B2").unwrap(), "hello");
        assert_eq!(wb.undo_labels(), vec!["paste 2 row(s)"]);

        wb.clear_range(0, (1, 1, 0, 0)).unwrap(); // reversed corners are fine
        assert_eq!(wb.range_snapshot(0, 0, 0, 9, 9).unwrap().len(), 0);
        wb.undo().unwrap();
        assert_eq!(wb.display_value(0, "A2").unwrap(), "3");
        wb.undo().unwrap();
        assert_eq!(wb.range_snapshot(0, 0, 0, 9, 9).unwrap().len(), 0);
        wb.redo().unwrap();
        assert_eq!(wb.display_value(0, "B2").unwrap(), "hello");

        // block partly outside the sheet is clipped, not an error
        wb.paste_text_block(0, (9, 9), &block).unwrap();
        assert_eq!(wb.display_value(0, "J10").unwrap(), "1");
    }

    // ------------------------------------------------------- cut / paste

    #[test]
    fn move_cells_rewires_references_like_the_desktop() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "5").unwrap();
        wb.edit_cell(0, "B1", "=A1*2").unwrap();
        wb.edit_cell(0, "C1", "=SUM(A1:A3)").unwrap();
        // cut A1, paste at D4
        wb.move_cells(0, (0, 0, 0, 0), (3, 3)).unwrap();
        assert_eq!(wb.cell_input(0, "D4").unwrap(), "5");
        assert_eq!(wb.cell_input(0, "A1").unwrap(), "0");
        assert_eq!(wb.formula(0, "B1").unwrap(), "B1=D4*2"); // reference followed the cell
        assert_eq!(wb.display_value(0, "B1").unwrap(), "10");
        assert_eq!(wb.display_value(0, "C1").unwrap(), "0"); // range formula recalculated
        wb.edit_cell(0, "D4", "7").unwrap();
        assert_eq!(wb.display_value(0, "B1").unwrap(), "14");

        // undo restores everything, including the rewritten formula
        wb.undo().unwrap();
        wb.undo().unwrap();
        assert_eq!(wb.formula(0, "B1").unwrap(), "B1=A1*2");
        assert_eq!(wb.display_value(0, "A1").unwrap(), "5");
        assert_eq!(wb.display_value(0, "C1").unwrap(), "5");
        assert_eq!(wb.cell_input(0, "D4").unwrap(), "");
        wb.redo().unwrap();
        assert_eq!(wb.formula(0, "B1").unwrap(), "B1=D4*2");
    }

    #[test]
    fn move_cells_moves_formulas_and_blocks() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "1").unwrap();
        wb.edit_cell(0, "A2", "2").unwrap();
        wb.edit_cell(0, "B1", "=A1+A2").unwrap();
        wb.edit_cell(0, "B2", "=B1*2").unwrap();
        // move the 2x2 block one column to the right (overlapping)
        wb.move_cells(0, (0, 0, 1, 1), (0, 1)).unwrap();
        assert_eq!(wb.cell_input(0, "B1").unwrap(), "1");
        assert_eq!(wb.cell_input(0, "B2").unwrap(), "2");
        assert_eq!(wb.formula(0, "C1").unwrap(), "C1=B1+B2");
        assert_eq!(wb.formula(0, "C2").unwrap(), "C2=C1*2");
        assert_eq!(wb.display_value(0, "C2").unwrap(), "6");
        assert_eq!(wb.cell_input(0, "A1").unwrap(), "0");
        // desktop quirk kept: a formula referencing the overwritten target
        // becomes an error constant
        wb.edit_cell(0, "E1", "9").unwrap();
        wb.edit_cell(0, "F1", "=E1").unwrap();
        wb.edit_cell(0, "G1", "3").unwrap();
        wb.move_cells(0, (0, 6, 0, 6), (0, 4)).unwrap(); // G1 -> E1
        assert_eq!(wb.display_value(0, "E1").unwrap(), "3");
        assert_eq!(wb.display_value(0, "F1").unwrap(), "Err");
        assert!(wb.move_cells(0, (0, 0, 0, 0), (9, 9)).is_ok());
        assert!(wb.move_cells(0, (9, 9, 9, 9), (9, 10)).is_err());
    }

    #[test]
    fn copy_values_pastes_values_not_formulas() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "4").unwrap();
        wb.edit_cell(0, "A2", "=A1*2").unwrap();
        wb.edit_cell_string(0, "A3", "txt");
        wb.edit_cell(0, "B1", "0").unwrap();
        wb.edit_cell(0, "A4", "=1/B1").unwrap(); // Err
        wb.edit_cell(0, "C1", "=C2+1").unwrap(); // references the paste target
        wb.copy_values(0, (0, 0, 3, 0), (0, 2)).unwrap(); // A1:A4 -> C1:C4
        assert_eq!(wb.cell_input(0, "C1").unwrap(), "4"); // C1's formula replaced by a constant
        assert_eq!(wb.cell_input(0, "C2").unwrap(), "8"); // value, not =A1*2
        assert_eq!(wb.display_value(0, "C3").unwrap(), "txt");
        assert_eq!(wb.display_value(0, "C4").unwrap(), "Err");
        wb.edit_cell(0, "A1", "100").unwrap();
        assert_eq!(wb.display_value(0, "C2").unwrap(), "8"); // no link to the source
        // blanks paste as blanks
        wb.copy_values(0, (5, 5, 5, 5), (0, 2)).unwrap();
        assert_eq!(wb.cell_input(0, "C1").unwrap(), "");
        wb.undo().unwrap();
        assert_eq!(wb.cell_input(0, "C1").unwrap(), "4");
        wb.undo().unwrap();
        wb.undo().unwrap();
        assert_eq!(wb.formula(0, "C1").unwrap(), "C1=C2+1");
    }

    impl Workbook {
        fn edit_cell_string(&mut self, index: usize, reference: &str, text: &str) {
            self.edit_cell(index, reference, &format!("\"{}\"", text))
                .unwrap();
        }
    }

    // ------------------------------------------------- find / replace

    #[test]
    fn find_and_replace_matches_constants_only() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "10").unwrap();
        wb.edit_cell(0, "A2", "=A1").unwrap(); // displays 10 but is a formula
        wb.edit_cell(0, "B1", "10").unwrap();
        wb.edit_cell_string(0, "C1", "10");
        wb.edit_cell_string(0, "D1", "ten");
        let found = wb.find(0, "10", false).unwrap();
        assert_eq!(
            found
                .iter()
                .map(|c| c.reference.as_str())
                .collect::<Vec<_>>(),
            vec!["A1", "B1", "C1", "A2"]
        );
        assert_eq!(wb.find(0, "TEN", false).unwrap().len(), 1);
        assert!(wb.find(0, "", false).unwrap().is_empty());

        let out = wb.find_and_replace(0, "10", "20").unwrap();
        assert_eq!((out.matched, out.replaced), (3, 3));
        assert_eq!(wb.display_value(0, "A1").unwrap(), "20");
        assert_eq!(wb.display_value(0, "A2").unwrap(), "20"); // dependent recalculated
        assert_eq!(wb.formula(0, "A2").unwrap(), "A2=A1"); // formula untouched
        assert_eq!(wb.cell_input(0, "C1").unwrap(), "\"20\""); // text stays text
        wb.undo().unwrap();
        assert_eq!(wb.display_value(0, "A1").unwrap(), "10");
        assert_eq!(wb.display_value(0, "C1").unwrap(), "10");

        // replacing a number with non-numeric text is rejected by the engine
        let out = wb.find_and_replace(0, "10", "abc").unwrap();
        assert_eq!((out.matched, out.replaced), (3, 1)); // only the text cell
        let out = wb.find_and_replace(0, "nothing", "x").unwrap();
        assert_eq!(out.matched, 0);
    }

    // ---------------------------------------------------------- sort

    #[test]
    fn sort_rows_by_column_keeps_dependents_in_sync() {
        let mut wb = wb();
        for (r, (a, b)) in [(3, "x"), (1, "y"), (2, "z")].iter().enumerate() {
            wb.edit_cell(0, &format!("A{}", r + 1), &a.to_string())
                .unwrap();
            wb.edit_cell_string(0, &format!("B{}", r + 1), b);
        }
        wb.edit_cell(0, "D1", "=A1*10").unwrap();
        let r = wb.sort_range(0, (0, 0, 2, 1), "A", true).unwrap();
        assert!(r.ok, "{}", r.status);
        let col = |wb: &Workbook, c: &str| -> Vec<String> {
            (1..=3)
                .map(|r| wb.display_value(0, &format!("{}{}", c, r)).unwrap())
                .collect()
        };
        assert_eq!(col(&wb, "A"), vec!["1", "2", "3"]);
        assert_eq!(col(&wb, "B"), vec!["y", "z", "x"]);
        assert_eq!(wb.display_value(0, "D1").unwrap(), "10"); // D1 sees the new A1
        wb.undo().unwrap();
        assert_eq!(col(&wb, "A"), vec!["3", "1", "2"]);
        assert_eq!(wb.display_value(0, "D1").unwrap(), "30");
        wb.redo().unwrap();
        assert_eq!(col(&wb, "B"), vec!["y", "z", "x"]);

        let r = wb.sort_range(0, (0, 0, 2, 1), "A", false).unwrap();
        assert!(r.ok);
        assert_eq!(col(&wb, "A"), vec!["3", "2", "1"]);
    }

    #[test]
    fn sort_columns_by_row_and_validation_messages() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "2").unwrap();
        wb.edit_cell(0, "B1", "1").unwrap();
        wb.edit_cell(0, "C1", "3").unwrap();
        wb.edit_cell(0, "A2", "20").unwrap();
        wb.edit_cell(0, "C2", "30").unwrap();
        assert!(wb.sort_range(0, (0, 0, 1, 2), "1", true).unwrap().ok);
        let row = |wb: &Workbook, r: i32| -> Vec<String> {
            ["A", "B", "C"]
                .iter()
                .map(|c| wb.display_value(0, &format!("{}{}", c, r)).unwrap())
                .collect()
        };
        assert_eq!(row(&wb, 1), vec!["1", "2", "3"]);
        assert_eq!(row(&wb, 2), vec!["0", "20", "30"]);
        assert_eq!(wb.cell_input(0, "A2").unwrap(), ""); // blanks stay blank

        assert_eq!(
            wb.sort_range(0, (2, 0, 0, 2), "A", true).unwrap().status,
            "wrong range"
        );
        assert_eq!(
            wb.sort_range(0, (0, 0, 1, 2), "Z", true).unwrap().status,
            "sorted column out of range"
        );
        assert_eq!(
            wb.sort_range(0, (0, 0, 1, 2), "9", true).unwrap().status,
            "sorted row out of range"
        );
        wb.edit_cell_string(0, "B1", "s");
        assert_eq!(
            wb.sort_range(0, (0, 0, 1, 2), "1", true).unwrap().status,
            "sorted row has string"
        );
        assert_eq!(
            wb.sort_range(0, (0, 0, 1, 2), "B", true).unwrap().status,
            "sorted column has string"
        );
        wb.edit_cell(0, "B1", "=C1").unwrap();
        assert_eq!(
            wb.sort_range(0, (0, 0, 1, 2), "A", true).unwrap().status,
            "range has constraints"
        );
        assert!(wb.sort_range(0, (0, 0, 50, 50), "A", true).unwrap().status == "wrong range");
    }

    #[test]
    fn sort_command_syntax() {
        assert_eq!(
            parse_sort_command("SORT(A1:B5;A;asc)"),
            Some(SortSpec {
                rect: (0, 0, 4, 1),
                key: "A".into(),
                ascending: true
            })
        );
        assert_eq!(
            parse_sort_command(" SORT( C2 : D3 ; 2 ; desc ) "),
            Some(SortSpec {
                rect: (1, 2, 2, 3),
                key: "2".into(),
                ascending: false
            })
        );
        assert!(parse_sort_command("SORT(A1:B5;A)").is_none());
        let mut wb = wb();
        wb.edit_cell(0, "A1", "2").unwrap();
        wb.edit_cell(0, "A2", "1").unwrap();
        assert!(wb.sort_command(0, "SORT(A1:A2;A;asc)").unwrap().unwrap().ok);
        assert_eq!(wb.display_value(0, "A1").unwrap(), "1");
        assert!(wb.sort_command(0, "A1=5").unwrap().is_none());
    }

    // ------------------------------------------------ file compatibility

    #[test]
    fn old_desktop_290_format_with_full_buul_grid_still_loads() {
        let mut wb = Workbook::with_default_sheet(3, 3).unwrap();
        wb.set_cell(0, "A1", "1").unwrap();
        wb.set_cell(0, "C3", "SUM(A1:B2)").unwrap();
        // Hand-build the legacy layout: buul as a full boolean grid.
        let mut value: serde_json::Value = serde_json::from_str(&wb.to_json().unwrap()).unwrap();
        let grid = serde_json::json!([
            [true, true, false],
            [true, true, false],
            [false, false, false]
        ]);
        value[0]["sheet"]["buul"] = grid;
        let legacy = serde_json::to_string(&value).unwrap();
        assert!(legacy.contains("[[true,true,false]"));

        let mut back = Workbook::from_json(&legacy).unwrap();
        assert_eq!(back.sheet(0).unwrap().sheet.buul[1][1], true);
        assert_eq!(back.display_value(0, "C3").unwrap(), "1");
        back.set_cell(0, "B2", "5").unwrap();
        assert_eq!(back.display_value(0, "C3").unwrap(), "6");

        // the new compact form is small even for a big blank sheet
        let big = Workbook::with_default_sheet(1000, 1000).unwrap();
        assert!(big.to_json().unwrap().len() < 200);
    }

    // ------------------------------------------------------------ formatting

    fn bold() -> FormatPatch {
        FormatPatch {
            bold: Some(true),
            ..FormatPatch::default()
        }
    }

    #[test]
    fn formatting_is_stored_per_cell_and_never_touches_values() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "5").unwrap();
        wb.edit_cell(0, "B1", "=A1*2").unwrap();
        let patch = FormatPatch {
            bold: Some(true),
            fill: Some(Some("#ffff00".into())),
            align: Some(Some(Align::Center)),
            ..FormatPatch::default()
        };
        assert!(wb.set_format(0, (0, 0, 0, 2), &patch).unwrap().ok); // A1:C1, C1 has no content
        let f = wb.cell_format(0, "A1").unwrap();
        assert!(f.bold && f.fill.as_deref() == Some("#ffff00") && f.align == Some(Align::Center));
        assert_eq!(wb.cell_format(0, "C1").unwrap(), f);
        assert_eq!(wb.cell_format(0, "D1").unwrap(), CellFormat::default());
        assert_eq!(wb.display_value(0, "B1").unwrap(), "10");
        assert_eq!(wb.formula(0, "B1").unwrap(), "B1=A1*2");

        // patches only touch the given properties; null clears
        let patch = FormatPatch {
            bold: Some(false),
            fill: Some(None),
            font_size: Some(Some(18)),
            ..FormatPatch::default()
        };
        wb.set_format(0, (0, 0, 0, 0), &patch).unwrap();
        let f = wb.cell_format(0, "A1").unwrap();
        assert!(
            !f.bold
                && f.fill.is_none()
                && f.font_size == Some(18)
                && f.align == Some(Align::Center)
        );

        // clear_all removes the entry entirely
        let clear = FormatPatch {
            clear_all: true,
            ..FormatPatch::default()
        };
        wb.set_format(0, (0, 0, 0, 2), &clear).unwrap();
        assert!(wb.sheet(0).unwrap().sheet.formats.is_empty());

        // format-only cells appear in snapshots (so the grid can paint them)
        wb.set_format(0, (5, 5, 5, 5), &bold()).unwrap();
        let snap = wb.range_snapshot(0, 0, 0, 9, 9).unwrap();
        let f6 = snap.iter().find(|c| c.reference == "F6").unwrap();
        assert_eq!(f6.display, "");
        assert!(f6.format.as_ref().unwrap().bold);
        assert!(
            snap.iter()
                .find(|c| c.reference == "A1")
                .unwrap()
                .format
                .is_none()
        );
        // clearing the content keeps the formatting (like other spreadsheets)
        wb.edit_cell(0, "F6", "x").unwrap();
        wb.clear_cell(0, "F6").unwrap();
        assert!(wb.cell_format(0, "F6").unwrap().bold);
    }

    #[test]
    fn formatting_patch_json_distinguishes_missing_from_null() {
        let p: FormatPatch = serde_json::from_str(r#"{"bold":true,"fill":null}"#).unwrap();
        assert_eq!(p.bold, Some(true));
        assert_eq!(p.fill, Some(None));
        assert_eq!(p.color, None);
        let p: FormatPatch =
            serde_json::from_str(r##"{"color":"#ff0000","align":"right","fontSize":12}"##).unwrap();
        assert_eq!(p.color, Some(Some("#ff0000".into())));
        assert_eq!(p.align, Some(Some(Align::Right)));
        assert_eq!(p.font_size, Some(Some(12)));
    }

    #[test]
    fn formatting_is_undoable_and_saved() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "1").unwrap();
        wb.set_format(0, (0, 0, 0, 0), &bold()).unwrap();
        assert_eq!(wb.undo_labels().last().unwrap(), "format 1 cell(s)");
        wb.undo().unwrap();
        assert!(!wb.cell_format(0, "A1").unwrap().bold);
        assert_eq!(wb.display_value(0, "A1").unwrap(), "1"); // content untouched by the replay
        wb.redo().unwrap();
        assert!(wb.cell_format(0, "A1").unwrap().bold);
        // a no-op patch records nothing
        let n = wb.undo_labels().len();
        wb.set_format(0, (0, 0, 0, 0), &bold()).unwrap();
        assert_eq!(wb.undo_labels().len(), n);

        let json = wb.to_json().unwrap();
        assert!(json.contains("\"bold\":true"));
        let back = Workbook::from_json(&json).unwrap();
        assert!(back.cell_format(0, "A1").unwrap().bold);
        // files without the formats / charts fields (older builds) still load
        let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
        value[0]["sheet"].as_object_mut().unwrap().remove("formats");
        value[0]["sheet"].as_object_mut().unwrap().remove("charts");
        let older = Workbook::from_json(&value.to_string()).unwrap();
        assert_eq!(older.cell_format(0, "A1").unwrap(), CellFormat::default());
        assert_eq!(older.display_value(0, "A1").unwrap(), "1");
    }

    #[test]
    fn formatting_follows_cut_copy_and_sort() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "3").unwrap();
        wb.edit_cell(0, "A2", "1").unwrap();
        wb.set_format(0, (0, 0, 0, 0), &bold()).unwrap(); // A1 bold
        // cut A1 -> C1: format moves
        wb.move_cells(0, (0, 0, 0, 0), (0, 2)).unwrap();
        assert!(wb.cell_format(0, "C1").unwrap().bold);
        assert!(!wb.cell_format(0, "A1").unwrap().bold);
        wb.undo().unwrap();
        assert!(wb.cell_format(0, "A1").unwrap().bold);
        assert!(!wb.cell_format(0, "C1").unwrap().bold);
        // copy A1 -> D1: format copied, source keeps it
        wb.copy_values(0, (0, 0, 0, 0), (0, 3)).unwrap();
        assert!(wb.cell_format(0, "D1").unwrap().bold && wb.cell_format(0, "A1").unwrap().bold);
        // sort A1:A2 ascending: the bold "3" moves to row 2 with its value
        assert!(wb.sort_range(0, (0, 0, 1, 0), "A", true).unwrap().ok);
        assert_eq!(wb.display_value(0, "A2").unwrap(), "3");
        assert!(wb.cell_format(0, "A2").unwrap().bold);
        assert!(!wb.cell_format(0, "A1").unwrap().bold);
        wb.undo().unwrap();
        assert!(wb.cell_format(0, "A1").unwrap().bold && !wb.cell_format(0, "A2").unwrap().bold);
    }

    // ------------------------------------------------------------ charts

    fn chart_spec() -> ChartSpec {
        ChartSpec {
            id: 0,
            kind: crate::charts::ChartKind::Line,
            title: "Sales".into(),
            x_label: String::new(),
            y_label: String::new(),
            range: crate::charts::RangeRef {
                row1: 0,
                col1: 0,
                row2: 2,
                col2: 1,
            },
            series_in_rows: false,
            header_labels: false,
            first_series_is_x: false,
        }
    }

    #[test]
    fn charts_are_stored_with_the_sheet_and_read_live_values() {
        let mut wb = wb();
        wb.edit_cell(0, "A1", "1").unwrap();
        wb.edit_cell(0, "A2", "2").unwrap();
        wb.edit_cell(0, "A3", "=A1+A2").unwrap();
        wb.edit_cell(0, "B1", "10").unwrap();
        let id = wb.add_chart(0, chart_spec()).unwrap();
        assert_eq!(id, 1);
        assert_eq!(wb.add_chart(0, chart_spec()).unwrap(), 2);
        let data = wb.chart_data(0, &wb.charts(0).unwrap()[0]).unwrap();
        assert_eq!(data.series[0].values, vec![Some(1.0), Some(2.0), Some(3.0)]);
        assert_eq!(
            data.series[1].values,
            vec![Some(10.0), Some(0.0), Some(0.0)]
        );
        wb.edit_cell(0, "A1", "5").unwrap();
        let data = wb.chart_data(0, &wb.charts(0).unwrap()[0]).unwrap();
        assert_eq!(data.series[0].values, vec![Some(5.0), Some(2.0), Some(7.0)]); // formula recalculated

        let mut spec = wb.charts(0).unwrap()[1].clone();
        spec.title = "Updated".into();
        spec.kind = crate::charts::ChartKind::Bar;
        wb.update_chart(0, spec).unwrap();
        assert_eq!(wb.charts(0).unwrap()[1].title, "Updated");
        let json = wb.to_json().unwrap();
        let mut back = Workbook::from_json(&json).unwrap();
        assert_eq!(back.charts(0).unwrap().len(), 2);
        assert_eq!(
            back.charts(0).unwrap()[1].kind,
            crate::charts::ChartKind::Bar
        );
        back.remove_chart(0, 1).unwrap();
        assert_eq!(back.charts(0).unwrap().len(), 1);
        assert!(matches!(
            back.remove_chart(0, 99),
            Err(CoreError::InvalidChart(99))
        ));
        assert!(back.update_chart(0, chart_spec()).is_err()); // id 0 does not exist
        assert!(wb.charts(5).is_err());
    }

    /// The shared cases also run against wasm (web/src/parity.test.ts) and
    /// the browser UI (web/e2e/parity.spec.ts); this is the reference run.
    #[test]
    fn shared_parity_cases_through_the_native_core() {
        let json = include_str!("../../../tests/parity-cases.json");
        let doc: serde_json::Value = serde_json::from_str(json).unwrap();
        let mut wb = Workbook::with_default_sheet(
            doc["rows"].as_i64().unwrap() as i32,
            doc["cols"].as_i64().unwrap() as i32,
        )
        .unwrap();
        let mut checks = 0;
        for (i, step) in doc["steps"].as_array().unwrap().iter().enumerate() {
            if let Some(cmd) = step["cmd"].as_str() {
                let result = wb.execute_command(0, cmd).unwrap();
                assert_eq!(
                    result.status,
                    step["status"].as_str().unwrap(),
                    "step {} `{}`",
                    i,
                    cmd
                );
                if let Some(sleep) = step["sleepSeconds"].as_i64() {
                    assert_eq!(
                        result.sleep_seconds as i64, sleep,
                        "step {} `{}` sleep",
                        i, cmd
                    );
                }
            } else {
                for (reference, expected) in step["check"].as_object().unwrap() {
                    assert_eq!(
                        wb.display_value(0, reference).unwrap(),
                        expected.as_str().unwrap(),
                        "step {} cell {}",
                        i,
                        reference
                    );
                    checks += 1;
                }
            }
        }
        assert!(checks > 50);
        // Final grid in the CLI's print format (compare with the desktop CLI
        // fed the same commands): `cargo test shared_parity -- --nocapture`
        println!(
            "PARITY-GRID\n{}",
            crate::sheet_functions::render_sheet_text(0, 0, 10, 10, &wb.sheet(0).unwrap().sheet)
        );
    }

    #[test]
    fn navigation_commands_touch_the_viewport_only() {
        let mut wb = wb();
        let r = wb.execute_command(0, "scroll_toC3").unwrap();
        assert!(r.ok);
        assert_eq!((wb.view().row_start, wb.view().col_start), (2, 2));
        let r = wb.execute_command(0, "q").unwrap();
        assert!(r.ok); // reported, never exits the process
    }
}
