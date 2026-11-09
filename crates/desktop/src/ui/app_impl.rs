use crate::ui::themes::THEMES;
use crate::ui::themes::Theme;
use crate::ui::utils;
use spreadsheet_core::parser;
use spreadsheet_core::sheet_functions::OpCode;
use spreadsheet_core::sheet_functions::{self, Cell, Sheet};
use spreadsheet_core::sheet_functions::{CellInfo, col_num_to_col_name, get_or_create_cell};
use std::string::String;

/// A sheet + tab name. This is `spreadsheet_core::NamedSheet`; the alias keeps
/// the existing desktop code (and the `.290` JSON format) unchanged.
pub use spreadsheet_core::workbook::NamedSheet as Sheets;

#[derive(Clone, Debug)]
pub enum Action {
    // NewSheet {
    //     sheet: Sheets,
    //     index: usize,
    // },
    // DeleteSheet {
    //     sheet: Sheets,
    //     index: usize,
    // },
    // ClearSheet {
    //     sheet_index: usize,
    //     old_data: Vec<Vec<Cell>>,
    // },
    // Deleted {
    //     sheet_index: usize,
    //     row: i16,
    //     col: i16,
    //     deleted_cell: Cell,
    // },
    Inserted {
        sheet_index: usize,
        row: i16,
        col: i16,
        previous_cell: Cell,
    },
    CutAction {
        sheet_index: usize,
        row1: i16,
        col1: i16,
        previous_cell1: Cell,
        row2: i16,
        col2: i16,
        previous_cell2: Cell,
    }, //1 -> cut from here , 2-> pasted here
    FindAndReplace {
        sheet_index: usize,
        changes: Vec<(usize, usize, Cell, Cell, String)>, // (row, col, old_cell, new_cell, command)
    },
    Sort {
        sheet_index: usize,
        changes: Vec<Vec<Cell>>,
        row1: i32,
        col1: i32,
        row2: i32,
        col2: i32,
    },
}

#[derive(Clone, PartialEq)]
pub enum CutCopy {
    Cut,
    Copied,
}

#[derive(Debug, PartialEq)]
pub enum Mode {
    Normal,
    Insert,
}

#[derive(Debug, PartialEq)]
pub enum Menu {
    Save,
    Open,
    Theme,
    NewSheet,
    DeleteSheet,
    Font,
    FindAndReplace,
    PlotGraph,
    Sort,
    SelectCell,
    None,
}

pub struct SpreadsheetApp {
    pub sheets: Vec<Sheets>,
    pub current_sheet_index: usize,
    pub formula: String,
    pub status: String,
    pub current_font_index: usize,
    pub mode: Mode,
    pub selected_cell: Option<(usize, usize)>,
    pub row_start: i32,
    pub col_start: i32,
    pub time: f32,
    pub timer: i32,
    pub editing_value: String,
    pub is_editing: bool,
    pub show_menu: Menu,
    pub save_filename: String,
    pub open_filename: String,
    pub theme: Theme,
    pub new_sheet_rows: String,
    pub new_sheet_cols: String,
    pub new_sheet_name: String,
    pub undo_stack: Vec<Action>,
    pub redo_stack: Vec<Action>,
    pub clipboard: Option<(Cell, CutCopy, i16, i16)>,
    pub find_text: String,
    pub replace_text: String,
    pub cut_copied_cell: Option<(i16, i16)>,
    pub plot_column1: String,
    pub plot_column2: String,
    pub plot_row_start: String,
    pub plot_row_end: String,
    pub show_plot: bool,
    pub sort_range_start: String,
    pub sort_range_end: String,
    pub sort_col_row: String,
    pub ascending: bool,
    pub input_select_cell: String,
}

impl SpreadsheetApp {
    pub fn new(sheets: Vec<Sheets>) -> Self {
        Self {
            sheets,
            current_sheet_index: 0,
            formula: String::new(),
            status: "ok".into(),
            mode: Mode::Normal,
            selected_cell: None,
            current_font_index: 0,
            row_start: 0,
            col_start: 0,
            time: 0.0,
            timer: 0,
            editing_value: String::new(),
            is_editing: false,
            show_menu: Menu::None,
            save_filename: String::from("sheet"),
            open_filename: String::new(),
            theme: THEMES[0].clone(),
            new_sheet_rows: String::new(),
            new_sheet_cols: String::new(),
            new_sheet_name: String::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            clipboard: None,
            find_text: String::new(),
            replace_text: String::new(),
            cut_copied_cell: None,
            plot_column1: String::new(),
            plot_column2: String::new(),
            plot_row_start: String::new(),
            plot_row_end: String::new(),
            show_plot: false,
            sort_range_start: String::new(),
            sort_range_end: String::new(),
            sort_col_row: String::new(),
            ascending: true,
            input_select_cell: String::new(),
        }
    }

    pub fn find_and_replace(&mut self) {
        self.clear_clipboard();
        let mut sheet = &mut self.sheets[self.current_sheet_index].sheet;
        let mut changes = Vec::new();
        let find_text = self.find_text.clone();
        let replace_text = self.replace_text.clone();
        let mut is_num;
        let num_row = sheet.rows;
        let num_col = sheet.cols;
        for ((row, col), cell) in sheet.data.clone().iter() {
            let cell_content = cell.display_value();
            is_num = !cell.is_error && cell.string.is_none();

            if cell_content == find_text
                && (cell.op_code == OpCode::NoConstraint || cell.op_code == OpCode::String)
            {
                let old_cell = cell.clone();
                let command;
                if !is_num {
                    command = format!(
                        "{}{}=\"{}\"",
                        col_num_to_col_name(*col as i32),
                        row + 1,
                        replace_text
                    );
                } else {
                    command = format!(
                        "{}{}={}",
                        col_num_to_col_name(*col as i32),
                        row + 1,
                        replace_text
                    );
                }
                sheet_functions::remove_dependency(
                    &CellInfo {
                        row: *row as i16,
                        col: *col as i16,
                    },
                    sheet,
                );
                parser::parse_command(
                    &command,
                    &mut self.row_start,
                    &mut self.col_start,
                    &mut self.time,
                    &mut self.status,
                    &num_row,
                    &num_col,
                    &mut sheet,
                    &mut true,
                );
                let new_cell = get_or_create_cell(sheet, *row as i32, *col as i32).clone();
                changes.push((*row as usize, *col as usize, old_cell, new_cell, command));
            }
        }

        if !changes.is_empty() {
            let len = changes.len();
            self.undo_stack.push(Action::FindAndReplace {
                sheet_index: self.current_sheet_index,
                changes,
            });
            self.redo_stack.clear();
            let sorted =
                sheet_functions::topological_sort(&mut std::collections::HashMap::new(), sheet);
            for i in sorted {
                let r = i % 1000;
                let c = i / 1000;
                sheet_functions::recalculate(sheet, r as usize, c as usize, &mut self.timer);
            }
            self.status = format!("Replaced {} instances", len);
        } else {
            self.status = "No matches found".to_string();
        }
    }

    pub fn undo(&mut self) {
        if self.undo_stack.is_empty() {
            self.status = "Nothing to undo".to_string();
            return;
        }
        self.clear_clipboard();
        if let Some(action) = self.undo_stack.pop() {
            match action {
                Action::Inserted {
                    sheet_index,
                    row,
                    col,
                    previous_cell,
                } => {
                    utils::insert_undo_redo(sheet_index, row, col, previous_cell, self, false);
                    self.status = "Undone cell edit".to_string();
                }
                Action::CutAction {
                    sheet_index,
                    row1,
                    col1,
                    previous_cell1,
                    row2,
                    col2,
                    previous_cell2,
                } => {
                    utils::cut_undo_redo(
                        sheet_index,
                        row1,
                        col1,
                        previous_cell1,
                        row2,
                        col2,
                        previous_cell2,
                        self,
                        false,
                    );
                    self.status = "Undone cut".to_string();
                }
                // Action::Deleted { sheet_index, row, col, deleted_cell } => {
                //     // utils::delete_cell_and_update_dependencies(sheet_index, row, col, deleted_cell, &mut self);
                //     self.status = "Undone delete".to_string();
                // }
                Action::FindAndReplace {
                    sheet_index,
                    changes,
                } => {
                    let mut sheet = &mut self.sheets[sheet_index].sheet;
                    let sheet1 = sheet.clone();
                    let mut redo_changes = Vec::new();
                    for (row, col, old_cell, _, command) in changes {
                        let original_cmd = if old_cell.string.is_some() {
                            format!(
                                "{}{}=\"{}\"",
                                col_num_to_col_name(col as i32),
                                row + 1,
                                old_cell.string.as_ref().unwrap()
                            )
                        } else if old_cell.is_error {
                            format!("{}{}=Err", col_num_to_col_name(col as i32), row + 1)
                        } else {
                            format!(
                                "{}{}=\"{}\"",
                                col_num_to_col_name(col as i32),
                                row + 1,
                                old_cell.value
                            )
                        };
                        sheet_functions::remove_dependency(
                            &CellInfo {
                                row: row as i16,
                                col: col as i16,
                            },
                            sheet,
                        );
                        parser::parse_command(
                            &original_cmd,
                            &mut self.row_start,
                            &mut self.col_start,
                            &mut self.time,
                            &mut self.status,
                            &sheet1.rows,
                            &sheet1.cols,
                            &mut sheet,
                            &mut true,
                        );
                        let new_cell_after_undo =
                            get_or_create_cell(sheet, row as i32, col as i32).clone();
                        redo_changes.push((
                            row,
                            col,
                            old_cell.clone(),
                            new_cell_after_undo,
                            command,
                        ));
                    }
                    self.redo_stack.push(Action::FindAndReplace {
                        sheet_index,
                        changes: redo_changes,
                    });
                    let sorted = sheet_functions::topological_sort(
                        &mut std::collections::HashMap::new(),
                        sheet,
                    );
                    for i in sorted {
                        let r = i % 1000;
                        let c = i / 1000;
                        sheet_functions::recalculate(
                            sheet,
                            r as usize,
                            c as usize,
                            &mut self.timer,
                        );
                    }
                    self.status = "Undone find and replace".to_string();
                }
                Action::Sort {
                    sheet_index,
                    changes,
                    row1,
                    col1,
                    row2,
                    col2,
                } => {
                    utils::sort_add_to_stack(sheet_index, col1, row1, col2, row2, self, true);
                    for i in row1..(row2 + 1) {
                        for j in col1..(col2 + 1) {
                            let cell = get_or_create_cell(
                                &mut self.sheets[self.current_sheet_index].sheet,
                                i,
                                j,
                            );
                            *cell = changes[(i - row1) as usize][(j - col1) as usize].clone();
                        }
                    }
                    self.status = "Undone sort".to_string();
                }
            }
        } else {
            self.status = "Nothing to undo".to_string();
        }
    }

    pub fn redo(&mut self) {
        if self.redo_stack.is_empty() {
            self.status = "Nothing to redo".to_string();
            return;
        }
        self.clear_clipboard();
        if let Some(action) = self.redo_stack.pop() {
            match action {
                Action::Inserted {
                    sheet_index,
                    row,
                    col,
                    previous_cell,
                } => {
                    utils::insert_undo_redo(sheet_index, row, col, previous_cell, self, true);
                    self.status = "Redone cell edit".to_string();
                }
                Action::CutAction {
                    sheet_index,
                    row1,
                    col1,
                    previous_cell1,
                    row2,
                    col2,
                    previous_cell2,
                } => {
                    utils::cut_undo_redo(
                        sheet_index,
                        row1,
                        col1,
                        previous_cell1,
                        row2,
                        col2,
                        previous_cell2,
                        self,
                        true,
                    );
                    self.status = "Redone cut".to_string();
                }
                Action::FindAndReplace {
                    sheet_index,
                    changes,
                } => {
                    let mut sheet = &mut self.sheets[sheet_index].sheet;
                    let sheet1 = sheet.clone();
                    let mut undo_changes = Vec::new();
                    for (row, col, _, _, command) in changes {
                        let prev_cell = get_or_create_cell(sheet, row as i32, col as i32).clone();
                        sheet_functions::remove_dependency(
                            &CellInfo {
                                row: row as i16,
                                col: col as i16,
                            },
                            sheet,
                        );
                        parser::parse_command(
                            &command,
                            &mut self.row_start,
                            &mut self.col_start,
                            &mut self.time,
                            &mut self.status,
                            &sheet1.rows,
                            &sheet1.cols,
                            &mut sheet,
                            &mut true,
                        );
                        let new_cell_after_redo =
                            get_or_create_cell(sheet, row as i32, col as i32).clone();
                        undo_changes.push((row, col, prev_cell, new_cell_after_redo, command));
                    }
                    self.undo_stack.push(Action::FindAndReplace {
                        sheet_index,
                        changes: undo_changes,
                    });
                    let sorted = sheet_functions::topological_sort(
                        &mut std::collections::HashMap::new(),
                        sheet,
                    );
                    for i in sorted {
                        let r = i % 1000;
                        let c = i / 1000;
                        sheet_functions::recalculate(
                            sheet,
                            r as usize,
                            c as usize,
                            &mut self.timer,
                        );
                    }
                    self.status = "Redone find and replace".to_string();
                }
                Action::Sort {
                    sheet_index,
                    changes,
                    row1,
                    col1,
                    row2,
                    col2,
                } => {
                    utils::sort_add_to_stack(sheet_index, col1, row1, col2, row2, self, false);
                    for i in row1..(row2 + 1) {
                        for j in col1..(col2 + 1) {
                            let cell =
                                get_or_create_cell(&mut self.sheets[sheet_index].sheet, i, j);
                            *cell = changes[(i - row1) as usize][(j - col1) as usize].clone();
                        }
                    }
                    self.status = "Redone sort".to_string();
                }
            }
        } else {
            self.status = "Nothing to redo".to_string();
        }
    }

    pub fn clear_clipboard(&mut self) {
        self.clipboard = None;
        self.cut_copied_cell = None;
    }

    pub fn create_new_sheet(&mut self, rows: i32, cols: i32) {
        let new_sheet = Sheet::new(rows, cols);
        let sheet_struct = Sheets {
            sheet: new_sheet.clone(),
            name: self.new_sheet_name.clone(),
        };
        self.sheets.push(sheet_struct.clone());
        let new_index = self.sheets.len() - 1;
        self.current_sheet_index = new_index;
        self.redo_stack.clear();
        self.show_menu = Menu::None;
        self.status = String::from("Sheet created successfully");
        self.new_sheet_name = String::new();
    }
}
