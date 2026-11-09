use crate::ui::app_impl::{Action, Mode, SpreadsheetApp};
use egui::Id;
use spreadsheet_core::parser;
use spreadsheet_core::sheet_functions::{col_num_to_col_name, get_or_create_cell};

pub fn show_spreadsheet(
    app: &mut SpreadsheetApp,
    ctx: &egui::Context,
    visible_rows: &i32,
    visible_cols: &i32,
) -> egui::InnerResponse<()> {
    egui::CentralPanel::default().show(ctx, |ui| {
        let cell_size = egui::vec2(120.0, 30.0);

        let scroll_delta = ui.input(|i| i.smooth_scroll_delta);
        if scroll_delta.y != 0.0 {
            let scroll_rows = (scroll_delta.y / 30.0).round() as i32;
            app.row_start = (app.row_start - scroll_rows)
                .max(0)
                .min(app.sheets[app.current_sheet_index].sheet.rows - 1);
        }
        if scroll_delta.x != 0.0 {
            let scroll_cols = (scroll_delta.x / 30.0).round() as i32;
            app.col_start = (app.col_start - scroll_cols)
                .max(0)
                .min(app.sheets[app.current_sheet_index].sheet.cols - 1);
        }

        egui::Grid::new("header_grid")
            .min_col_width(cell_size.x)
            .min_row_height(cell_size.y)
            .spacing([0.0, 0.0])
            .show(ui, |ui| {
                let corner_rect = ui.available_rect_before_wrap();
                ui.painter()
                    .rect_filled(corner_rect, 0.0, app.theme.header_bg);
                ui.painter().rect_stroke(
                    corner_rect,
                    0.0,
                    egui::Stroke::new(1.0, app.theme.grid_line_color),
                );
                ui.add_sized(
                    cell_size,
                    egui::Label::new(
                        egui::RichText::new("")
                            .color(app.theme.header_text_color)
                            .text_style(egui::TextStyle::Heading),
                    ),
                );

                for c in app.col_start
                    ..(app.col_start + visible_cols)
                        .min(app.sheets[app.current_sheet_index].sheet.cols)
                {
                    let header_rect = ui.available_rect_before_wrap();
                    ui.painter()
                        .rect_filled(header_rect, 0.0, app.theme.header_bg);
                    ui.painter().rect_stroke(
                        header_rect,
                        0.0,
                        egui::Stroke::new(1.0, app.theme.grid_line_color),
                    );

                    ui.with_layout(
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.add_sized(
                                cell_size,
                                egui::Label::new(
                                    egui::RichText::new(col_num_to_col_name(c))
                                        .strong()
                                        .color(app.theme.header_text_color)
                                        .text_style(egui::TextStyle::Heading),
                                ),
                            );
                        },
                    );
                }
                ui.end_row();
            });

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("sheet_grid")
                    .min_col_width(cell_size.x)
                    .min_row_height(cell_size.y)
                    .spacing([0.0, 0.0])
                    .show(ui, |ui| {
                        for r in app.row_start
                            ..(app.row_start + visible_rows)
                                .min(app.sheets[app.current_sheet_index].sheet.rows)
                        {
                            let row_header_rect = ui.available_rect_before_wrap();
                            ui.painter()
                                .rect_filled(row_header_rect, 0.0, app.theme.header_bg);
                            ui.painter().rect_stroke(
                                row_header_rect,
                                0.0,
                                egui::Stroke::new(1.0, app.theme.grid_line_color),
                            );

                            ui.with_layout(
                                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                                |ui| {
                                    ui.add_sized(
                                        cell_size,
                                        egui::Label::new(
                                            egui::RichText::new((r + 1).to_string())
                                                .strong()
                                                .color(app.theme.header_text_color)
                                                .text_style(egui::TextStyle::Heading),
                                        ),
                                    );
                                },
                            );

                            for c in app.col_start
                                ..(app.col_start + visible_cols)
                                    .min(app.sheets[app.current_sheet_index].sheet.cols)
                            {
                                let cell = get_or_create_cell(
                                    &mut app.sheets[app.current_sheet_index].sheet,
                                    r as i32,
                                    c as i32,
                                );
                                let display = cell.display_value();

                                let is_cut = app.cut_copied_cell == Some((r as i16, c as i16));
                                let is_sel = app.selected_cell == Some((r as usize, c as usize));

                                let rect = ui.available_rect_before_wrap();

                                ui.painter().rect_filled(
                                    rect,
                                    0.0,
                                    if is_sel || is_cut {
                                        app.theme.selected_cell_bg
                                    } else {
                                        app.theme.cell_bg
                                    },
                                );

                                ui.painter().rect_stroke(
                                    rect,
                                    0.0,
                                    egui::Stroke::new(1.0, app.theme.grid_line_color),
                                );

                                if is_sel && app.mode == Mode::Insert {
                                    if !app.is_editing {
                                        app.editing_value = display.clone();
                                        app.is_editing = true;
                                    }

                                    let id = Id::new("cell_edit");
                                    if app.is_editing {
                                        ui.memory_mut(|mem| mem.request_focus(id));
                                    }

                                    let edit = ui.add_sized(
                                        cell_size,
                                        egui::TextEdit::singleline(&mut app.editing_value)
                                            .id(id)
                                            .frame(false)
                                            .desired_width(cell_size.x)
                                            .text_color(app.theme.text_color)
                                            .cursor_at_end(true),
                                    );

                                    if edit.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    {
                                        app.clear_clipboard();
                                        let col_name = col_num_to_col_name(c);
                                        let row_str = (r + 1).to_string();
                                        let cmd = format!(
                                            "{}{}={}",
                                            col_name, row_str, app.editing_value
                                        );
                                        let old_cell = get_or_create_cell(
                                            &mut app.sheets[app.current_sheet_index].sheet,
                                            r as i32,
                                            c as i32,
                                        )
                                        .clone();
                                        let sheet_rows =
                                            app.sheets[app.current_sheet_index].sheet.rows;
                                        let sheet_cols =
                                            app.sheets[app.current_sheet_index].sheet.cols;
                                        parser::parse_command(
                                            &cmd,
                                            &mut app.row_start,
                                            &mut app.col_start,
                                            &mut app.time,
                                            &mut app.status,
                                            &sheet_rows,
                                            &sheet_cols,
                                            &mut app.sheets[app.current_sheet_index].sheet,
                                            &mut true,
                                        );

                                        app.undo_stack.push(Action::Inserted {
                                            sheet_index: app.current_sheet_index,
                                            row: r as i16,
                                            col: c as i16,
                                            previous_cell: old_cell,
                                        });
                                        app.redo_stack.clear();
                                        app.is_editing = false;
                                        app.editing_value.clear();
                                    }

                                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        app.mode = Mode::Normal;
                                        app.is_editing = false;
                                        app.selected_cell = None;
                                        app.editing_value.clear();
                                    }

                                    if ui.input(|i| i.pointer.any_click()) {
                                        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
                                        if let Some(pos) = pointer_pos {
                                            if !rect.contains(pos) {
                                                app.is_editing = false;
                                                app.editing_value.clear();
                                            }
                                        }
                                    }
                                } else {
                                    let response = ui.add_sized(
                                        cell_size,
                                        egui::Label::new(
                                            egui::RichText::new(display)
                                                .color(app.theme.text_color),
                                        )
                                        .sense(egui::Sense::click()),
                                    );

                                    if response.clicked() {
                                        if app.is_editing {
                                            app.is_editing = false;
                                            app.editing_value.clear();
                                        }

                                        app.selected_cell = Some((r as usize, c as usize));

                                        if response.clicked_by(egui::PointerButton::Primary)
                                            && response.double_clicked()
                                        {
                                            app.mode = Mode::Insert;
                                        }
                                    }
                                }
                            }
                            ui.end_row();
                        }
                    });
            });

        ui.horizontal(|ui| {
            if ui.button("◀").clicked() {
                app.col_start = (app.col_start - 1).max(0);
            }

            let progress = app.col_start as f32
                / (app.sheets[app.current_sheet_index].sheet.cols - visible_cols).max(1) as f32;
            let mut progress_bar = progress;
            if ui
                .add(egui::Slider::new(&mut progress_bar, 0.0..=1.0).text("Scroll"))
                .changed()
            {
                app.col_start = ((app.sheets[app.current_sheet_index].sheet.cols - visible_cols)
                    .max(1) as f32
                    * progress_bar) as i32;
                app.col_start = app
                    .col_start
                    .max(0)
                    .min(app.sheets[app.current_sheet_index].sheet.cols - 1);
            }

            if ui.button("▶").clicked() {
                app.col_start = (app.col_start + 1)
                    .min((app.sheets[app.current_sheet_index].sheet.cols - visible_cols).max(0));
            }
        });
    })
}
