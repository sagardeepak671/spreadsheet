// use std::f64::consts::E;
use crate::sheet_functions::OpCode;
use crate::sheet_functions::OpCode::*;
use crate::sheet_functions::Sheet;
use std::string::String;

/// Computes the sum of values in a specified range of cells in the spreadsheet.
///
/// # Arguments
/// * `sheet` - The spreadsheet containing the cell data.
/// * `val_row1` - The starting row index of the range (0-based).
/// * `c1` - The starting column index of the range (0-based).
/// * `val_row2` - The ending row index of the range (0-based).
/// * `c2` - The ending column index of the range (0-based).
///
/// # Returns
/// The sum of all cell values in the specified range. Non-existent cells are treated as having a value of 0.
pub fn sum(sheet: &Sheet, val_row1: i16, c1: i16, val_row2: i16, c2: i16) -> i32 {
    let mut a = 0;
    let hash_map_size = sheet.data.len();
    let range_size = (val_row2 - val_row1 + 1) as usize * (c2 - c1 + 1) as usize;
    if hash_map_size > range_size {
        for i in val_row1..=val_row2 {
            for j in c1..=c2 {
                let value = sheet.data.get(&(i, j)).map_or(0, |cell| cell.value); // Use 0 if the cell is not in the map
                a += value;
            }
        }
    } else {
        for (row, col) in sheet.data.keys() {
            if row >= &val_row1 && row <= &val_row2 && col >= &c1 && col <= &c2 {
                let value = sheet.data.get(&(*row, *col)).map_or(0, |cell| cell.value);
                a += value;
            }
        }
    }
    a
}

/// Finds the minimum value in a specified range of cells in the spreadsheet.
///
/// # Arguments
/// * `sheet` - The spreadsheet containing the cell data.
/// * `val_row1` - The starting row index of the range (0-based).
/// * `c1` - The starting column index of the range (0-based).
/// * `val_row2` - The ending row index of the range (0-based).
/// * `c2` - The ending column index of the range (0-based).
///
/// # Returns
/// The minimum value in the specified range. Non-existent cells are treated as having a value of 0.
pub fn min(sheet: &Sheet, val_row1: i16, c1: i16, val_row2: i16, c2: i16) -> i32 {
    let mut a = i32::MAX;
    let hash_map_size = sheet.data.len();
    let range_size = (val_row2 - val_row1 + 1) as usize * (c2 - c1 + 1) as usize;
    if hash_map_size >= range_size {
        for i in val_row1..=val_row2 {
            for j in c1..=c2 {
                let value = sheet.data.get(&(i, j)).map_or(0, |cell| cell.value); // Use 0 if the cell is not in the map
                if value < a {
                    a = value;
                }
            }
        }
    } else {
        let mut count = 0;
        for (row, col) in sheet.data.keys() {
            if row >= &val_row1 && row <= &val_row2 && col >= &c1 && col <= &c2 {
                let value = sheet.data.get(&(*row, *col)).map_or(0, |cell| cell.value);
                if value < a {
                    a = value;
                }
                count += 1;
            }
        }
        if count != range_size {
            a = if a > 0 { 0 } else { a };
        }
    }
    a
}

/// Finds the maximum value in a specified range of cells in the spreadsheet.
///
/// # Arguments
/// * `sheet` - The spreadsheet containing the cell data.
/// * `val_row1` - The starting row index of the range (0-based).
/// * `c1` - The starting column index of the range (0-based).
/// * `val_row2` - The ending row index of the range (0-based).
/// * `c2` - The ending column index of the range (0-based).
///
/// # Returns
/// The maximum value in the specified range. Non-existent cells are treated as having a value of 0.
/// Returns `i32::MIN` if the range is empty.
pub fn max(sheet: &Sheet, val_row1: i16, c1: i16, val_row2: i16, c2: i16) -> i32 {
    let mut a = i32::MIN;
    let hash_map_size = sheet.data.len();
    let range_size = (val_row2 - val_row1 + 1) as usize * (c2 - c1 + 1) as usize;
    if hash_map_size >= range_size {
        for i in val_row1..=val_row2 {
            for j in c1..=c2 {
                let value = sheet.data.get(&(i, j)).map_or(0, |cell| cell.value); // Use 0 if the cell is not in the map
                if value > a {
                    a = value;
                }
            }
        }
    } else {
        let mut count = 0;
        for (row, col) in sheet.data.keys() {
            if row >= &val_row1 && row <= &val_row2 && col >= &c1 && col <= &c2 {
                let value = sheet.data.get(&(*row, *col)).map_or(0, |cell| cell.value);
                if value > a {
                    a = value;
                }
                count += 1;
            }
        }
        if count != range_size {
            a = if a < 0 { 0 } else { a };
        }
    }
    a
}

/// Computes the average of values in a specified range of cells in the spreadsheet.
///
/// # Arguments
/// * `sheet` - The spreadsheet containing the cell data.
/// * `val_row1` - The starting row index of the range (0-based).
/// * `c1` - The starting column index of the range (0-based).
/// * `val_row2` - The ending row index of the range (0-based).
/// * `c2` - The ending column index of the range (0-based).
///
/// # Returns
/// The average of all cell values in the specified range, rounded down to the nearest integer.
/// Returns -1 if the range is empty.
pub fn avg(sheet: &Sheet, val_row1: i16, c1: i16, val_row2: i16, c2: i16) -> i32 {
    let total = sum(sheet, val_row1, c1, val_row2, c2);
    let row2 = val_row2 as i32;
    let col2 = c2 as i32;
    let row1 = val_row1 as i32;
    let col1 = c1 as i32;

    let count = (row2 - row1 + 1) * (col2 - col1 + 1);
    if count > 0 { total / count } else { -1 }
}

/// Computes the standard deviation of values in a specified range of cells in the spreadsheet.
///
/// # Arguments
/// * `sheet` - The spreadsheet containing the cell data.
/// * `val_row1` - The starting row index of the range (0-based).
/// * `c1` - The starting column index of the range (0-based).
/// * `val_row2` - The ending row index of the range (0-based).
/// * `c2` - The ending column index of the range (0-based).
///
/// # Returns
/// The standard deviation of cell values in the specified range, rounded to the nearest integer.
/// Returns 0 if the range is empty.
pub fn stdev(sheet: &Sheet, val_row1: i16, c1: i16, val_row2: i16, c2: i16) -> i32 {
    let avg_val = avg(sheet, val_row1, c1, val_row2, c2);
    let mut total: i64 = 0;
    let count = (val_row2 - val_row1 + 1) as i32 * (c2 - c1 + 1) as i32;
    let hash_map_size = sheet.data.len();
    if hash_map_size > count as usize {
        for i in val_row1..=val_row2 {
            for j in c1..=c2 {
                let value = sheet.data.get(&(i, j)).map_or(0, |cell| cell.value); // Use 0 if the cell is not in the map
                let diff: i64 = (value - avg_val) as i64;
                total += diff * diff;
            }
        }
    } else {
        for (row, col) in sheet.data.keys() {
            if row >= &val_row1 && row <= &val_row2 && col >= &c1 && col <= &c2 {
                let value = sheet.data.get(&(*row, *col)).map_or(0, |cell| cell.value);
                let diff: i64 = (value - avg_val) as i64;
                total += diff * diff;
            }
        }
    }
    let std = (total as f64 / count as f64).sqrt();
    std.round() as i32
}

/// Computes the result of a binary operation between two values based on the specified `OpCode`.
///
/// # Arguments
/// * `op_code` - The operation to perform (e.g., addition, subtraction, multiplication, division).
/// * `cell_value` - The first operand (typically a cell's value).
/// * `cell_value2` - The second operand (a cell's value or a constant).
/// * `status` - A mutable string to store error messages if the operation fails.
///
/// # Returns
/// A tuple containing:
/// * The computed result as an `i32`.
/// * A boolean indicating whether an error occurred (e.g., division by zero).
pub fn compute_cell(
    op_code: OpCode,
    cell_value: i32,
    cell_value2: i32,
    status: &mut String,
) -> (i32, bool) {
    match op_code {
        CellPlusCell | CellPlusConstant => (cell_value + cell_value2, false),
        CellMinusCell | CellMinusConstant => (cell_value - cell_value2, false),
        ConstantMinusCell => (cell_value2 - cell_value, false),
        CellTimesCell | CellTimesConstant => (cell_value * cell_value2, false),
        CellDivideCell | CellDivideConstant => {
            if cell_value2 == 0 {
                (-1, true)
            } else {
                (cell_value / cell_value2, false)
            }
        }
        ConstantDividesCell => {
            if cell_value == 0 {
                (-1, true)
            } else {
                (cell_value2 / cell_value, false)
            }
        }
        String => (cell_value, false),
        _ => {
            status.push_str("err");
            (-1, true)
        }
    }
}

/// Computes the result of a range-based function on a specified range of cells.
///
/// # Arguments
/// * `sheet` - The spreadsheet containing the cell data.
/// * `op_code` - The range-based operation to perform (e.g., `Sum`, `Min`, `Max`, `Avg`, `Stdev`).
/// * `row1` - The starting row index of the range (0-based).
/// * `col1` - The starting column index of the range (0-based).
/// * `row2` - The ending row index of the range (0-based).
/// * `col2` - The ending column index of the range (0-based).
/// * `status` - A mutable string to store error messages if the operation fails.
/// # Returns
/// The result of the range-based operation as an `i32`. Returns -1 if the range is invalid or the operation is unsupported.
pub fn compute_range_func(
    sheet: &Sheet,
    op_code: OpCode,
    row1: i16,
    col1: i16,
    row2: i16,
    col2: i16,
    status: &mut String,
) -> i32 {
    if col1 > col2 || row1 > row2 {
        status.push_str("err");
        return -1;
    }
    match op_code {
        Sum => sum(sheet, row1, col1, row2, col2),
        Min => min(sheet, row1, col1, row2, col2),
        Max => max(sheet, row1, col1, row2, col2),
        Avg => avg(sheet, row1, col1, row2, col2),
        Stdev => stdev(sheet, row1, col1, row2, col2),
        _ => -1,
    }
}
