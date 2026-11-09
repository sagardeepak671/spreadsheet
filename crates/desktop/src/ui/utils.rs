//! Desktop-only helpers: file I/O wrappers around the core's CSV / JSON
//! conversions, and the undo/redo bookkeeping for `SpreadsheetApp`.

use crate::ui::app_impl::Sheets;
use crate::ui::app_impl::{Action, SpreadsheetApp};
use regex::Regex;
use spreadsheet_core::csv;
use spreadsheet_core::dependency_ops::{recalculate_dependecy, sort_sheet};
use spreadsheet_core::sheet_functions::Cell;
use spreadsheet_core::sheet_functions::CellInfo;
use spreadsheet_core::sheet_functions::OpCode;
use spreadsheet_core::sheet_functions::Sheet;
use spreadsheet_core::sheet_functions::{self, get_or_create_cell};
use spreadsheet_core::workbook::Workbook;
use std::fs;
use std::string::String;

/// Writes `<filename>.csv` with the current values of `sheet`.
pub fn convert_to_csv(sheet: &Sheet, filename: &str) {
    let save_file_name = format!("{}.csv", filename);
    fs::write(save_file_name, csv::sheet_to_csv(sheet)).unwrap();
}

/// Loads a CSV file into a new sheet appended to the app.
pub fn open_csv(filename: &str, app: &mut SpreadsheetApp) -> String {
    let text = match fs::read_to_string(filename) {
        Ok(t) => t,
        Err(e) => return format!("Failed to open file: {}", e),
    };
    app.sheets.push(Sheets {
        sheet: csv::sheet_from_csv(&text),
        name: filename.to_string(),
    });
    app.current_sheet_index = app.sheets.len() - 1;
    app.redo_stack.clear();
    "CSV loaded successfully".to_string()
}

/// Saves every sheet (formulas + dependency graph) as JSON (`.290`).
pub fn save_all_sheets(sheets: &Vec<Sheets>, filename: &str) {
    let json = Workbook::from_sheets(sheets.clone())
        .to_json_pretty()
        .unwrap();
    fs::write(filename, json).unwrap();
}

/// Loads a `.290` file written by [`save_all_sheets`] (or by the browser).
pub fn load_all_sheets(filename: &str) -> Vec<Sheets> {
    let json = fs::read_to_string(filename).unwrap();
    Workbook::from_json(&json).unwrap().into_sheets()
}

pub fn insert_undo_redo(
    sheet_index: usize,
    row: i16,
    col: i16,
    previous_cell: Cell,
    app: &mut SpreadsheetApp,
    redo: bool,
) {
    let curr_cell =
        get_or_create_cell(&mut app.sheets[sheet_index].sheet, row as i32, col as i32).clone();
    sheet_functions::remove_dependency(
        &CellInfo {
            row: row as i16,
            col: col as i16,
        },
        &mut app.sheets[sheet_index].sheet,
    );
    let old_cell = previous_cell.clone();
    if old_cell.cell1.row != -1 && old_cell.cell1.col != -1 {
        get_or_create_cell(
            &mut app.sheets[sheet_index].sheet,
            old_cell.cell1.row as i32,
            old_cell.cell1.col as i32,
        )
        .dependencies
        .insert(col as i32 * 1000 + row as i32);
    }
    if old_cell.cell2.row != -1 && old_cell.cell2.col != -1 {
        get_or_create_cell(
            &mut app.sheets[sheet_index].sheet,
            old_cell.cell2.row as i32,
            old_cell.cell2.col as i32,
        )
        .dependencies
        .insert(col as i32 * 1000 + row as i32);
    }
    *get_or_create_cell(&mut app.sheets[sheet_index].sheet, row as i32, col as i32) = old_cell;

    if !redo {
        app.redo_stack.push(Action::Inserted {
            sheet_index,
            row,
            col,
            previous_cell: curr_cell,
        });
    } else {
        app.undo_stack.push(Action::Inserted {
            sheet_index,
            row,
            col,
            previous_cell: curr_cell,
        });
    }
    recalculate_dependecy(
        CellInfo {
            row: row as i16,
            col: col as i16,
        },
        &mut app.sheets[sheet_index].sheet,
    );
}

pub fn cut_undo_redo(
    sheet_index: usize,
    row1: i16,
    col1: i16,
    previous_cell1: Cell,
    row2: i16,
    col2: i16,
    previous_cell2: Cell,
    app: &mut SpreadsheetApp,
    redo: bool,
) {
    let curr_cell1 =
        get_or_create_cell(&mut app.sheets[sheet_index].sheet, row1 as i32, col1 as i32).clone();
    let curr_cell2 =
        get_or_create_cell(&mut app.sheets[sheet_index].sheet, row2 as i32, col2 as i32).clone();
    sheet_functions::remove_dependency(
        &CellInfo {
            row: row1 as i16,
            col: col1 as i16,
        },
        &mut app.sheets[sheet_index].sheet,
    );
    sheet_functions::remove_dependency(
        &CellInfo {
            row: row2 as i16,
            col: col2 as i16,
        },
        &mut app.sheets[sheet_index].sheet,
    );
    *get_or_create_cell(&mut app.sheets[sheet_index].sheet, row1 as i32, col1 as i32) =
        previous_cell1;
    *get_or_create_cell(&mut app.sheets[sheet_index].sheet, row2 as i32, col2 as i32) =
        previous_cell2;
    let cell1_1 = get_or_create_cell(&mut app.sheets[sheet_index].sheet, row1 as i32, col1 as i32)
        .cell1
        .clone();
    let cell1_2 = get_or_create_cell(&mut app.sheets[sheet_index].sheet, row1 as i32, col1 as i32)
        .cell2
        .clone();
    let cell2_1 = get_or_create_cell(&mut app.sheets[sheet_index].sheet, row2 as i32, col2 as i32)
        .cell1
        .clone();
    let cell2_2 = get_or_create_cell(&mut app.sheets[sheet_index].sheet, row2 as i32, col2 as i32)
        .cell2
        .clone();
    if cell1_1.row != -1 && cell1_1.col != -1 {
        get_or_create_cell(
            &mut app.sheets[sheet_index].sheet,
            cell1_1.row as i32,
            cell1_1.col as i32,
        )
        .dependencies
        .insert(col1 as i32 * 1000 + row1 as i32);
    }
    if cell1_2.row != -1 && cell1_2.col != -1 {
        get_or_create_cell(
            &mut app.sheets[sheet_index].sheet,
            cell1_2.row as i32,
            cell1_2.col as i32,
        )
        .dependencies
        .insert(col1 as i32 * 1000 + row1 as i32);
    }
    if cell2_1.row != -1 && cell2_1.col != -1 {
        get_or_create_cell(
            &mut app.sheets[sheet_index].sheet,
            cell2_1.row as i32,
            cell2_1.col as i32,
        )
        .dependencies
        .insert(col2 as i32 * 1000 + row2 as i32);
    }
    if cell2_2.row != -1 && cell2_2.col != -1 {
        get_or_create_cell(
            &mut app.sheets[sheet_index].sheet,
            cell2_2.row as i32,
            cell2_2.col as i32,
        )
        .dependencies
        .insert(col2 as i32 * 1000 + row2 as i32);
    }
    recalculate_dependecy(
        CellInfo {
            row: row1 as i16,
            col: col1 as i16,
        },
        &mut app.sheets[sheet_index].sheet,
    );
    recalculate_dependecy(
        CellInfo {
            row: row2 as i16,
            col: col2 as i16,
        },
        &mut app.sheets[sheet_index].sheet,
    );
    if !redo {
        app.redo_stack.push(Action::CutAction {
            sheet_index: sheet_index,
            row1: row1,
            col1: col1,
            previous_cell1: curr_cell1,
            row2: row2,
            col2: col2,
            previous_cell2: curr_cell2,
        });
    } else {
        app.undo_stack.push(Action::CutAction {
            sheet_index: sheet_index,
            row1: row1,
            col1: col1,
            previous_cell1: curr_cell1,
            row2: row2,
            col2: col2,
            previous_cell2: curr_cell2,
        });
    }
}

pub fn sort_add_to_stack(
    sheet_index: usize,
    col1: i32,
    row1: i32,
    col2: i32,
    row2: i32,
    app: &mut SpreadsheetApp,
    add_to_redo: bool,
) {
    let mut changes = Vec::new();
    for i in row1..(row2 + 1) {
        let mut temp = Vec::new();
        for j in col1..(col2 + 1) {
            temp.push(
                get_or_create_cell(&mut app.sheets[sheet_index].sheet, i as i32, j as i32).clone(),
            );
        }
        changes.push(temp);
    }
    if !add_to_redo {
        app.undo_stack.push(Action::Sort {
            sheet_index,
            changes,
            row1,
            col1,
            row2,
            col2,
        });
    } else {
        app.redo_stack.push(Action::Sort {
            sheet_index,
            changes,
            row1,
            col1,
            row2,
            col2,
        });
    }
}

pub fn sort_extension(
    col1: i32,
    row1: i32,
    col2: i32,
    row2: i32,
    sort_key: &str,
    is_column: bool,
    sort_order: &str,
    app: &mut SpreadsheetApp,
) {
    if (col2 < col1) || (row2 < row1) {
        app.status = String::from("wrong range");
        app.time = 0.0;
        return;
    }
    if is_column {
        let sort_col = sheet_functions::col_name_to_col_num(sort_key);
        if sort_col > col2 || sort_col < col1 {
            app.status = String::from("sorted column out of range");
            app.time = 0.0;
            return;
        }
        for i in row1..=row2 + 1 {
            if get_or_create_cell(
                &mut app.sheets[app.current_sheet_index].sheet,
                i as i32,
                sort_col as i32,
            )
            .string
            .is_some()
            {
                app.status = String::from("sorted column has string");
                app.time = 0.0;
                return;
            }
        }
    } else {
        let sort_row = sort_key.parse::<i32>().unwrap() - 1;
        if sort_row > row2 || sort_row < row1 {
            app.status = String::from("sorted row out of range");
            app.time = 0.0;
            return;
        }
        for i in col1..=col2 + 1 {
            if get_or_create_cell(
                &mut app.sheets[app.current_sheet_index].sheet,
                sort_row as i32,
                i as i32,
            )
            .string
            .is_some()
            {
                app.status = String::from("sorted row has string");
                app.time = 0.0;
                return;
            }
        }
    }
    for i in row1..=row2 + 1 {
        for j in col1..=col2 + 1 {
            if get_or_create_cell(
                &mut app.sheets[app.current_sheet_index].sheet,
                i as i32,
                j as i32,
            )
            .op_code
                != OpCode::NoConstraint
                && get_or_create_cell(
                    &mut app.sheets[app.current_sheet_index].sheet,
                    i as i32,
                    j as i32,
                )
                .op_code
                    != OpCode::String
            {
                app.status = String::from("range has constraints");
                app.time = 0.0;
                return;
            }
        }
    }
    sort_add_to_stack(app.current_sheet_index, col1, row1, col2, row2, app, false);
    sort_sheet(
        &mut app.sheets[app.current_sheet_index].sheet,
        col1,
        row1,
        col2,
        row2,
        sort_key,
        is_column,
        sort_order,
    );
}

pub fn sort_button_parser(app: &mut SpreadsheetApp, sort_asc: bool) {
    let re_cell = Regex::new(r"^([A-Z]+)(\d+)$").unwrap();
    if app.sort_col_row.is_empty()
        || app.sort_range_start.is_empty()
        || app.sort_range_end.is_empty()
    {
        app.status = "Please enter required data".to_string();
    } else {
        let col1;
        let row1;
        let col2;
        let row2;
        if let Some(caps) = re_cell.captures(&app.sort_range_start.to_string()) {
            let col_in = caps.get(1).unwrap().as_str();
            let row_in = caps.get(2).unwrap().as_str();
            col1 = sheet_functions::col_name_to_col_num(col_in);
            row1 = row_in.parse::<i32>().unwrap();
        } else {
            app.status = "Invalid range start".to_string();
            return;
        }
        if let Some(caps) = re_cell.captures(&app.sort_range_end.to_string()) {
            let col_in = caps.get(1).unwrap().as_str();
            let row_in = caps.get(2).unwrap().as_str();
            col2 = sheet_functions::col_name_to_col_num(col_in);
            row2 = row_in.parse::<i32>().unwrap();
        } else {
            app.status = "Invalid range end".to_string();
            return;
        }
        let is_column = app.sort_col_row.chars().all(|c| c.is_ascii_alphabetic());
        let sort_key = app.sort_col_row.clone();
        sort_extension(
            col1,
            row1 - 1,
            col2,
            row2 - 1,
            &sort_key,
            is_column,
            if sort_asc { "asc" } else { "desc" },
            app,
        );

        app.sort_col_row = String::new();
        app.sort_range_start = String::new();
        app.sort_range_end = String::new();
        app.status = "Sorting completed".to_string();
        app.time = 0.0;
    }
}
