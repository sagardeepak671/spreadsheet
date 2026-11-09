//! The command language of the spreadsheet (`A1=5`, `A1=B1+C1`,
//! `A1=SUM(A1:B2)`, `A1="text"`, `A1=SLEEP(3)`, plus the CLI navigation
//! commands `w/a/s/d`, `scroll_toA1`, `enable_output`, `disable_output`, `q`).
//!
//! Two entry points:
//!
//! * [`execute_command`] – pure: parses + applies the command to the sheet and
//!   *reports* what the host should do next (sleep N seconds, quit). Used by the
//!   wasm bridge and by [`Workbook`](crate::workbook::Workbook).
//! * [`parse_command`] – the legacy blocking wrapper with the original
//!   signature. It calls `execute_command`, then measures wall-clock time,
//!   performs the SLEEP, and exits the process on `q`. Used by the terminal CLI
//!   and the desktop GUI (and by the original test-suite).

use crate::sheet_functions;
use crate::sheet_functions::CellInfo;
use crate::sheet_functions::OpCode;
use crate::sheet_functions::OpCode::*;
use crate::sheet_functions::Sheet;
use crate::sheet_functions::col_name_to_col_num;
use crate::sheet_functions::get_or_create_cell;
use crate::sheet_functions::is_valid_cell;
use regex_lite::Regex;
use std::string::String;
use std::sync::LazyLock;

/// Scroll / output state of a text viewport (the 10x10 CLI window). Kept here
/// because the command language includes navigation commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Viewport {
    pub row_start: i32,
    pub col_start: i32,
    pub print_enabled: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport {
            row_start: 0,
            col_start: 0,
            print_enabled: true,
        }
    }
}

/// What happened while executing one command. The engine never sleeps or
/// exits by itself; the host decides what to do with this.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommandOutcome {
    /// Total seconds the command asked to sleep (`SLEEP(..)` cells, including
    /// dependent SLEEP cells that were recalculated).
    pub sleep_seconds: i32,
    /// The command was `q`.
    pub quit_requested: bool,
    /// `false` for pure navigation commands (`scroll_to`) where the original
    /// implementation reported a fixed `0.0` time instead of measuring.
    pub timed: bool,
}

struct CommandRegexes {
    scroll_to: Regex,
    cell_eq_int_op_int: Regex,
    cell_eq_cell_op_cell: Regex,
    cell_eq_int_op_cell: Regex,
    cell_eq_cell_op_int: Regex,
    cell_eq_func: Regex,
    cell_eq_int: Regex,
    cell_eq_cell: Regex,
    sleep_int: Regex,
    sleep_cell: Regex,
    string_cell: Regex,
}

// Compiled once per process instead of once per command.
static RE: LazyLock<CommandRegexes> = LazyLock::new(|| CommandRegexes {
    scroll_to: Regex::new(r"^scroll_to([A-Z]+)(\d+)$").unwrap(),
    cell_eq_int_op_int: Regex::new(r"^([A-Z]+)(\d+)=(\d+)([+\-*/])(\d+)$").unwrap(),
    cell_eq_cell_op_cell: Regex::new(r"^([A-Z]+)(\d+)=([A-Z]+)(\d+)([+\-*/])([A-Z]+)(\d+)$")
        .unwrap(),
    cell_eq_int_op_cell: Regex::new(r"^([A-Z]+)(\d+)=(\d+)([+\-*/])([A-Z]+)(\d+)$").unwrap(),
    cell_eq_cell_op_int: Regex::new(r"^([A-Z]+)(\d+)=([A-Z]+)(\d+)([+\-*/])(\d+)$").unwrap(),
    cell_eq_func: Regex::new(r"^([A-Z]+)(\d+)=([A-Z]+)\(([A-Z]+)(\d+):([A-Z]+)(\d+)\)$").unwrap(),
    cell_eq_int: Regex::new(r"^([A-Z]+)(\d+)=(-?\d+)$").unwrap(),
    cell_eq_cell: Regex::new(r"^([A-Z]+)(\d+)=([A-Z]+)(\d+)$").unwrap(),
    sleep_int: Regex::new(r"^([A-Z]+)(\d+)=SLEEP\((\d+)\)$").unwrap(),
    sleep_cell: Regex::new(r"^([A-Z]+)(\d+)=SLEEP\(([A-Z]+)(\d+)\)$").unwrap(),
    string_cell: Regex::new(r#"^([A-Z]+)(\d+)="(.*)"$"#).unwrap(),
});

/// Maps a character operator to the corresponding `OpCode` for operations involving a cell and a constant.
///
/// # Arguments
/// * `op_code` - The character representing the operator (`+`, `-`, `*`, `/`).
/// * `constopcell` - If `true`, indicates the constant is the first operand (e.g., constant / cell); otherwise, the cell is the first operand.
///
/// # Returns
/// The corresponding `OpCode` for the operation, or `NoConstraint` for constant operations (like cell = int, string)
pub fn get_op_code(op_code: char, constopcell: bool) -> OpCode {
    // function to get opcode for the case of int op cell or cell op int
    match op_code {
        '+' => CellPlusConstant,
        '-' => {
            if constopcell {
                ConstantMinusCell
            } else {
                CellMinusConstant
            }
        }
        '*' => CellTimesConstant,
        '/' => {
            if constopcell {
                ConstantDividesCell
            } else {
                CellDivideConstant
            }
        }
        _ => NoConstraint,
    }
}

/// Maps a character operator to the corresponding `OpCode` for operations between two cells.
pub fn get_op_code2(op_code: char) -> OpCode {
    match op_code {
        '+' => CellPlusCell,
        '-' => CellMinusCell,
        '*' => CellTimesCell,
        '/' => CellDivideCell,
        _ => NoConstraint,
    }
}

/// Maps a function name to the corresponding `OpCode` for range-based operations.
///
/// # Arguments
/// * `func` - The function name (e.g., "SUM", "MIN", "MAX", "AVG", "STDEV").
///
/// # Returns
/// The corresponding `OpCode` for the function, or `NoConstraint` if the opcode is not defined.
pub fn func_to_op_code(func: &str) -> OpCode {
    // function for getting opcode for the case of func(cell:cell)
    match func {
        "SUM" => Sum,
        "MIN" => Min,
        "MAX" => Max,
        "AVG" => Avg,
        "STDEV" => Stdev,
        _ => NoConstraint,
    }
}

/// Removes all space characters from a command string, except inside a
/// double-quoted string literal (`A1 = "hi there"` -> `A1="hi there"`).
///
/// # Arguments
/// * `command` - The mutable string to process.
fn remove_space(command: &mut String) {
    let mut in_quotes = false;
    *command = command
        .chars()
        .filter(|&c| {
            if c == '"' {
                in_quotes = !in_quotes;
            }
            c != ' ' || in_quotes
        })
        .collect();
}

/// Parses a command string and applies it to the sheet (pure: never sleeps,
/// never exits, never reads the clock).
///
/// # Arguments
/// * `command` - The command string to parse (e.g., "A1=5", "A1=A2+A3", "scroll_toA1").
/// * `view` - Viewport state mutated by the navigation commands.
/// * `status` - A mutable string to store the operation status (e.g., "ok", "err", "Invalid cmd").
/// * `total_rows` / `total_cols` - Dimensions of the sheet.
/// * `sheet` - The mutable spreadsheet to operate on.
///
/// # Returns
/// A [`CommandOutcome`] telling the host how long the command asked to sleep
/// and whether it asked to quit.
pub fn execute_command(
    command: &str,
    view: &mut Viewport,
    status: &mut String,
    total_rows: i32,
    total_cols: i32,
    sheet: &mut Sheet,
) -> CommandOutcome {
    let mut command = command.trim().to_string();
    let mut sleep_timer = 0;
    let mut outcome = CommandOutcome {
        sleep_seconds: 0,
        quit_requested: false,
        timed: true,
    };
    let re = &*RE;
    remove_space(&mut command);

    if command == "w" {
        view.row_start = std::cmp::max(0, view.row_start - 10);
        *status = String::from("ok");
    } else if command == "s" {
        view.row_start = std::cmp::min(view.row_start + 10, total_rows - 10);
        *status = String::from("ok");
    } else if command == "a" {
        view.col_start = std::cmp::max(0, view.col_start - 10);
        *status = String::from("ok");
    } else if command == "d" {
        view.col_start = std::cmp::min(view.col_start + 10, total_cols - 10);
        *status = String::from("ok");
    } else if command == "q" {
        outcome.quit_requested = true;
        *status = String::from("ok");
    } else if command == "enable_output" {
        view.print_enabled = true;
        *status = String::from("ok");
    } else if command == "disable_output" {
        view.print_enabled = false;
        *status = String::from("ok");
    }
    // scroll_to
    else if let Some(caps) = re.scroll_to.captures(&command) {
        let col_str = caps.get(1).unwrap().as_str(); // Extract column letters
        let row_num: i32 = caps.get(2).unwrap().as_str().parse().unwrap(); // Extract row number

        let c = col_name_to_col_num(col_str); // Convert column letters to number
        let r = row_num - 1; // Convert to 0-based index
        if is_valid_cell(r, c, total_rows, total_cols) {
            view.col_start = c;
            view.row_start = r;
            *status = String::from("ok");
        } else {
            *status = String::from("err");
        }
        outcome.timed = false;
        return outcome;
    }
    // Cell = String
    else if let Some(caps) = re.string_cell.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let value = caps.get(3).unwrap().as_str();
        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        if is_valid_cell(row, col, total_rows, total_cols) {
            let cell_entry = get_or_create_cell(sheet, row, col);
            // Update the cell's properties
            cell_entry.string = Some(value.to_string());
            cell_entry.value = 0;
            cell_entry.is_error = false;

            *status = String::from("ok");
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo { row: -1, col: -1 };
            let cell2 = CellInfo { row: -1, col: -1 };
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                String,
                sheet,
                status,
                &mut sleep_timer,
            );
        } else {
            *status = String::from("Invalid cmd");
        }
    }
    // Cell = int op int
    else if let Some(caps) = re.cell_eq_int_op_int.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let val1: i32 = caps.get(3).unwrap().as_str().parse().unwrap();
        let op = caps.get(4).unwrap().as_str().chars().next().unwrap();
        let val2: i32 = caps.get(5).unwrap().as_str().parse().unwrap();
        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        if is_valid_cell(row, col, total_rows, total_cols)
            && (op == '+' || op == '-' || op == '*' || op == '/')
        {
            let value;
            let mut calc_error = false;
            if op == '+' {
                value = val1 + val2;
            } else if op == '-' {
                value = val1 - val2;
            } else if op == '*' {
                value = val1 * val2;
            } else if val2 == 0 {
                calc_error = true;
                value = -1;
            } else {
                value = val1 / val2;
            }

            let cell_entry = get_or_create_cell(sheet, row, col);
            cell_entry.value = value;
            cell_entry.string = None;
            cell_entry.is_error = calc_error;

            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo { row: -1, col: -1 };
            let cell2 = CellInfo { row: -1, col: -1 };
            let op_code = NoConstraint;
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
            *status = String::from("ok");
        } else {
            *status = String::from("Invalid cmd");
        }
    }
    // Cell = cell op cell
    else if let Some(caps) = re.cell_eq_cell_op_cell.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let val_col1 = caps.get(3).unwrap().as_str();
        let val_row1: i32 = caps.get(4).unwrap().as_str().parse().unwrap();
        let op = caps.get(5).unwrap().as_str().chars().next().unwrap();
        let val_col2 = caps.get(6).unwrap().as_str();
        let val_row2: i32 = caps.get(7).unwrap().as_str().parse().unwrap();

        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        let col1 = col_name_to_col_num(val_col1);
        let col2 = col_name_to_col_num(val_col2);

        if is_valid_cell(row, col, total_rows, total_cols)
            && is_valid_cell(val_row1 - 1, col1, total_rows, total_cols)
            && is_valid_cell(val_row2 - 1, col2, total_rows, total_cols)
            && (op == '+' || op == '-' || op == '*' || op == '/')
        {
            let op_code = get_op_code2(op);
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo {
                row: val_row1 as i16 - 1,
                col: col1 as i16,
            };
            let cell2 = CellInfo {
                row: val_row2 as i16 - 1,
                col: col2 as i16,
            };
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
        } else {
            *status = String::from("Invalid cmd");
        }
    }
    // Cell = int op cell
    else if let Some(caps) = re.cell_eq_int_op_cell.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row = caps.get(2).unwrap().as_str().parse::<i32>().unwrap();
        let val1 = caps.get(3).unwrap().as_str().parse::<i32>().unwrap();
        let op = caps.get(4).unwrap().as_str().chars().next().unwrap();
        let val_col1 = caps.get(5).unwrap().as_str();
        let val_row1 = caps.get(6).unwrap().as_str().parse::<i32>().unwrap();

        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        let col1 = col_name_to_col_num(val_col1);

        if is_valid_cell(row, col, total_rows, total_cols)
            && (op == '+' || op == '-' || op == '*' || op == '/')
            && is_valid_cell(val_row1 - 1, col1, total_rows, total_cols)
        {
            // Splitting the constant into two 16 bit variables
            let const1 = val1 & 0xFFFF;
            let const2 = (val1 >> 16) & 0xFFFF;
            let op_code = get_op_code(op, true);
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo {
                row: val_row1 as i16 - 1,
                col: col1 as i16,
            };
            let cell2 = CellInfo {
                row: const2 as i16,
                col: const1 as i16,
            };
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
        } else {
            *status = String::from("Invalid cmd");
        }
    }
    // Cell = cell op int
    else if let Some(caps) = re.cell_eq_cell_op_int.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let val_col1 = caps.get(3).unwrap().as_str();
        let val_row1: i32 = caps.get(4).unwrap().as_str().parse().unwrap();
        let op = caps.get(5).unwrap().as_str().chars().next().unwrap();
        let val1: i32 = caps.get(6).unwrap().as_str().parse().unwrap();

        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        let col1 = col_name_to_col_num(val_col1);

        if is_valid_cell(row, col, total_rows, total_cols)
            && (op == '+' || op == '-' || op == '*' || op == '/')
            && is_valid_cell(val_row1 - 1, col1, total_rows, total_cols)
        {
            // Splitting the constant into two 16 bit variables
            let const1 = val1 & 0xFFFF;
            let const2 = (val1 >> 16) & 0xFFFF;
            let op_code = get_op_code(op, false);
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo {
                row: val_row1 as i16 - 1,
                col: col1 as i16,
            };
            let cell2 = CellInfo {
                row: const2 as i16,
                col: const1 as i16,
            };
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
        } else {
            *status = String::from("Invalid cmd");
        }
    }
    // Cell = func(cell:cell)
    else if let Some(caps) = re.cell_eq_func.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let func_name = caps.get(3).unwrap().as_str();
        let val_col1 = caps.get(4).unwrap().as_str();
        let val_row1: i32 = caps.get(5).unwrap().as_str().parse().unwrap();
        let val_col2 = caps.get(6).unwrap().as_str();
        let val_row2: i32 = caps.get(7).unwrap().as_str().parse().unwrap();

        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        let col1 = col_name_to_col_num(val_col1);
        let col2 = col_name_to_col_num(val_col2);

        let row1 = val_row1 - 1;
        let row2 = val_row2 - 1;

        let op_code = func_to_op_code(func_name);
        if is_valid_cell(row, col, total_rows, total_cols)
            && is_valid_cell(row1, col1, total_rows, total_cols)
            && is_valid_cell(row2, col2, total_rows, total_cols)
            && val_row1 <= val_row2
            && col1 <= col2
            && op_code != NoConstraint
        {
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo {
                row: row1 as i16,
                col: col1 as i16,
            };
            let cell2 = CellInfo {
                row: row2 as i16,
                col: col2 as i16,
            };
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
        } else {
            *status = String::from("Invalid cmd");
        }
    }
    // Cell = int
    else if let Some(caps) = re.cell_eq_int.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let val1: i32 = caps.get(3).unwrap().as_str().parse().unwrap_or_else(|_| {
            *status = String::from("Invalid cmd");
            0
        });

        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        if is_valid_cell(row, col, total_rows, total_cols) {
            let cell_entry = get_or_create_cell(sheet, row, col);
            cell_entry.string = None;
            cell_entry.value = val1;
            cell_entry.is_error = false;
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo { row: -1, col: -1 };
            let cell2 = CellInfo { row: -1, col: -1 };
            let op_code = NoConstraint;
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
            *status = String::from("ok");
        } else {
            *status = String::from("Invalid cmd");
        }
    }
    // Cell = cell
    else if let Some(caps) = re.cell_eq_cell.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let val_col1 = caps.get(3).unwrap().as_str();
        let val_row1: i32 = caps.get(4).unwrap().as_str().parse().unwrap();

        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        let col1 = col_name_to_col_num(val_col1);
        let row1 = val_row1 - 1;

        if is_valid_cell(row, col, total_rows, total_cols)
            && is_valid_cell(row1, col1, total_rows, total_cols)
        {
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo {
                row: row1 as i16,
                col: col1 as i16,
            };
            let cell2 = CellInfo { row: -1, col: -1 };
            let op_code = CellEqualsCell;
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
        } else {
            *status = String::from("Invalid cmd");
        }
    }
    // Cell = SLEEP(int)
    else if let Some(caps) = re.sleep_int.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let val1: i32 = caps.get(3).unwrap().as_str().parse().unwrap();

        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;

        if is_valid_cell(row, col, total_rows, total_cols) {
            *status = String::from("ok");
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo { row: -1, col: -1 };
            let cell2 = CellInfo { row: -1, col: -1 };
            let cell_entry = get_or_create_cell(sheet, row, col);
            cell_entry.string = None;
            cell_entry.value = val1;
            cell_entry.is_error = false;
            let op_code = NoConstraint;
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
            if val1 >= 0 {
                sleep_timer += val1;
            }
        } else {
            *status = String::from("err");
        }
    }
    // Cell = SLEEP(cell)
    else if let Some(caps) = re.sleep_cell.captures(&command) {
        let ref_col = caps.get(1).unwrap().as_str();
        let ref_row: i32 = caps.get(2).unwrap().as_str().parse().unwrap();
        let val_col1 = caps.get(3).unwrap().as_str();
        let val_row1: i32 = caps.get(4).unwrap().as_str().parse().unwrap();

        let col = col_name_to_col_num(ref_col);
        let row = ref_row - 1;
        let col1 = col_name_to_col_num(val_col1);
        let row1 = val_row1 - 1;

        if is_valid_cell(row, col, total_rows, total_cols)
            && is_valid_cell(row1, col1, total_rows, total_cols)
        {
            let cell = CellInfo {
                row: row as i16,
                col: col as i16,
            };
            let cell1 = CellInfo {
                row: row1 as i16,
                col: col1 as i16,
            };
            let cell2 = CellInfo { row: -1, col: -1 };
            let op_code = Sleep;
            sheet_functions::add_constraints(
                cell,
                cell1,
                cell2,
                op_code,
                sheet,
                status,
                &mut sleep_timer,
            );
        } else {
            *status = String::from("err");
        }
    } else {
        *status = String::from("Invalid cmd");
    }

    outcome.sleep_seconds = sleep_timer;
    outcome
}

/// Legacy blocking entry point (terminal CLI, desktop GUI, original tests).
///
/// Runs [`execute_command`] and then performs the host-side effects the
/// original implementation had: measures wall-clock time into `time`, sleeps
/// for `SLEEP(..)` commands, and exits the process on `q`.
///
/// Not for use from wasm (`Instant::now()` / `thread::sleep` are unavailable
/// on `wasm32-unknown-unknown`); the wasm bridge calls `execute_command`.
///
/// # Arguments
/// * `command` - The command string to parse (e.g., "A1=5", "A1=A2+A3", "scroll_toA1").
/// * `row_start` - A mutable reference to the starting row index for display.
/// * `col_start` - A mutable reference to the starting column index for display.
/// * `time` - A mutable reference to track the execution time or sleep duration.
/// * `status` - A mutable string to store the operation status (e.g., "ok", "err", "Invalid cmd").
/// * `total_rows` - The total number of rows in the spreadsheet.
/// * `total_cols` - The total number of columns in the spreadsheet.
/// * `sheet` - The mutable spreadsheet to operate on.
/// * `print_enabled` - A mutable boolean indicating whether output printing is enabled.
#[allow(clippy::too_many_arguments)]
pub fn parse_command(
    command: &str,
    row_start: &mut i32,
    col_start: &mut i32,
    time: &mut f32,
    status: &mut String,
    total_rows: &i32,
    total_cols: &i32,
    sheet: &mut Sheet,
    print_enabled: &mut bool,
) {
    let start_time = std::time::Instant::now();
    let mut view = Viewport {
        row_start: *row_start,
        col_start: *col_start,
        print_enabled: *print_enabled,
    };
    let outcome = execute_command(command, &mut view, status, *total_rows, *total_cols, sheet);
    *row_start = view.row_start;
    *col_start = view.col_start;
    *print_enabled = view.print_enabled;

    if outcome.quit_requested {
        std::process::exit(0);
    }
    if !outcome.timed {
        *time = 0.0;
        return;
    }

    let sleep_timer = outcome.sleep_seconds;
    let elapsed = start_time.elapsed();
    if elapsed.as_secs_f32() > sleep_timer as f32 {
        *time = elapsed.as_secs_f32();
    } else {
        *time = sleep_timer as f32;
        std::thread::sleep(std::time::Duration::from_secs(
            sleep_timer as u64 - elapsed.as_secs(),
        ));
    }
}
