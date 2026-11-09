//! Dependency-graph maintenance operations that the editing front-ends need
//! beyond plain "set a formula": rewiring references when a cell is moved
//! (cut/paste), forcing a recalculation cascade (undo/redo), and sorting a
//! rectangular range of constant cells.
//!
//! This file used to be `ui_sheet_functions.rs` inside the desktop binary, but
//! it contains no GUI code at all, so it now lives in the core where the wasm
//! bridge can use it too.

use crate::sheet_functions::OpCode;
use crate::sheet_functions::{Cell, Sheet};
use crate::sheet_functions::{
    CellInfo, add_to_tree, col_name_to_col_num, get_or_create_cell, recalculate, remove_dependency,
    topological_sort,
};

/// After a cell has been moved from `old` to `new`, walk every cell that
/// (transitively) depends on the new location and repoint `cell1`/`cell2`
/// references from the old coordinates to the new ones. Range formulas are
/// simply recalculated.
pub fn update_dependencies(
    old_cell_row: i16,
    old_cell_col: i16,
    new_cell_row: i16,
    new_cell_col: i16,
    sheet: &mut Sheet,
) {
    // Goes to the dependency set of cells depending on the old cell and updates references
    let mut avl_tree: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    avl_tree.insert(new_cell_col as i32 * 1000 + new_cell_row as i32, 0);
    add_to_tree(
        &mut avl_tree,
        CellInfo {
            row: new_cell_row,
            col: new_cell_col,
        },
        sheet,
    );

    let sorted = topological_sort(&mut avl_tree, sheet);

    for i in sorted.into_iter() {
        let row = i % 1000;
        let col = i / 1000;

        if row == new_cell_row as i32 && col == new_cell_col as i32 {
            continue;
        }

        if let Some(cell) = sheet.data.get_mut(&(row as i16, col as i16)) {
            if matches!(
                cell.op_code,
                OpCode::Sum | OpCode::Min | OpCode::Max | OpCode::Avg | OpCode::Stdev
            ) {
                //TODO handle range dependencies
                recalculate(sheet, row as usize, col as usize, &mut 0);
            } else {
                if cell.cell1.row == old_cell_row && cell.cell1.col == old_cell_col {
                    cell.cell1 = CellInfo {
                        row: new_cell_row,
                        col: new_cell_col,
                    };
                }
                if cell.cell2.row == old_cell_row && cell.cell2.col == old_cell_col {
                    cell.cell2 = CellInfo {
                        row: new_cell_row,
                        col: new_cell_col,
                    };
                }
            }
        }
    }
}

/// Removes the range / non-range dependents of `new_cell` (selected by
/// `del_range_dependencies`) and, for non-range dependents, repoints their
/// references from `(current_row, current_col)` to `(change_to_row, change_to_col)`.
#[allow(clippy::too_many_arguments)]
pub fn change_dependecy_set(
    new_cell: &mut Cell,
    sheet: &mut Sheet,
    del_range_dependencies: bool,
    change_to_row: i16,
    change_to_col: i16,
    current_row: i16,
    current_col: i16,
) {
    // Removes the range/non-range dependencies in the given dependency set
    for &dependency in new_cell.dependencies.clone().iter() {
        let row = dependency % 1000;
        let col = dependency / 1000;

        if let Some(dependent_cell) = sheet.data.get_mut(&(row as i16, col as i16)) {
            if matches!(
                dependent_cell.op_code,
                OpCode::Sum | OpCode::Min | OpCode::Max | OpCode::Avg | OpCode::Stdev
            ) && del_range_dependencies
            {
                new_cell.dependencies.remove(&(col * 1000 + row));
            } else if !del_range_dependencies
                && !matches!(
                    dependent_cell.op_code,
                    OpCode::Sum | OpCode::Min | OpCode::Max | OpCode::Avg | OpCode::Stdev
                )
            {
                new_cell.dependencies.remove(&(col * 1000 + row));

                if dependent_cell.cell1.row == current_row
                    && dependent_cell.cell1.col == current_col
                {
                    dependent_cell.cell1 = CellInfo {
                        row: change_to_row,
                        col: change_to_col,
                    };
                }
                if dependent_cell.cell2.row == current_row
                    && dependent_cell.cell2.col == current_col
                {
                    dependent_cell.cell2 = CellInfo {
                        row: change_to_row,
                        col: change_to_col,
                    };
                }
            }
        }
    }
}

/// Recalculates every cell that (transitively) depends on `curr_cell`, in
/// topological order. Used after a cell was overwritten wholesale (undo/redo,
/// paste) rather than through `add_constraints`.
pub fn recalculate_dependecy(curr_cell: CellInfo, sheet: &mut Sheet) {
    let mut dependency_set = std::collections::HashMap::new();
    dependency_set.insert(curr_cell.col as i32 * 1000 + curr_cell.row as i32, 0);
    add_to_tree(&mut dependency_set, curr_cell.clone(), sheet);
    let sorted = topological_sort(&mut dependency_set, sheet);
    for i in sorted.into_iter() {
        let row = i % 1000;
        let col = i / 1000;
        if row == curr_cell.row as i32 && col == curr_cell.col as i32 {
            continue;
        }
        recalculate(sheet, row as usize, col as usize, &mut 0);
    }
}

/// Computes the stable sort permutation used by [`sort_sheet`] without
/// touching the sheet.
///
/// Returns `order` such that line `k` of the sorted range is line
/// `order[k]` of the original range, where a "line" is a row when
/// `is_column` (rows reordered by the value in column `sort_key`) and a
/// column otherwise (columns reordered by the value in row `sort_key`,
/// 1-based). Missing cells count as 0; `sort_order` is `"asc"` or anything
/// else for descending.
#[allow(clippy::too_many_arguments)]
pub fn sort_order(
    sheet: &Sheet,
    col1: i32,
    row1: i32,
    col2: i32,
    row2: i32,
    sort_key: &str,
    is_column: bool,
    sort_order: &str,
) -> Vec<usize> {
    let value_at = |row: i32, col: i32| -> i32 {
        sheet
            .data
            .get(&(row as i16, col as i16))
            .map(|c| c.value)
            .unwrap_or(0)
    };
    let keys: Vec<i32> = if is_column {
        let col_num = col_name_to_col_num(sort_key);
        (row1..=row2).map(|r| value_at(r, col_num)).collect()
    } else {
        let row_num = sort_key.parse::<i32>().unwrap_or(1) - 1;
        (col1..=col2).map(|c| value_at(row_num, c)).collect()
    };
    let mut order: Vec<usize> = (0..keys.len()).collect();
    if sort_order == "asc" {
        order.sort_by_key(|&k| keys[k]);
    } else {
        order.sort_by_key(|&k| std::cmp::Reverse(keys[k]));
    }
    order
}

/// Sorts the rectangle `(row1,col1)..=(row2,col2)` in place by moving the
/// `Cell` structs (the desktop behaviour).
///
/// * `is_column == true`: rows are reordered by the value in column `sort_key`
///   (a column name such as `"B"`).
/// * `is_column == false`: columns are reordered by the value in row
///   `sort_key` (a 1-based row number such as `"3"`).
/// * `sort_order` is `"asc"` or anything else for descending.
///
/// Callers are expected to have verified that the range contains only
/// constant cells (see the desktop `sort_extension`). Missing cells are
/// materialised as empty cells.
#[allow(clippy::too_many_arguments)]
pub fn sort_sheet(
    sheet: &mut Sheet,
    col1: i32,
    row1: i32,
    col2: i32,
    row2: i32,
    sort_key: &str,
    is_column: bool,
    sort_order: &str,
) {
    let order = self::sort_order(
        sheet, col1, row1, col2, row2, sort_key, is_column, sort_order,
    );
    let cell_at = |sheet: &Sheet, row: i32, col: i32| -> Cell {
        sheet
            .data
            .get(&(row as i16, col as i16))
            .cloned()
            .unwrap_or_else(Cell::empty)
    };
    if is_column {
        let lines: Vec<Vec<Cell>> = (row1..=row2)
            .map(|r| (col1..=col2).map(|c| cell_at(sheet, r, c)).collect())
            .collect();
        for (k, &src) in order.iter().enumerate() {
            for (j, c) in (col1..=col2).enumerate() {
                sheet
                    .data
                    .insert(((row1 + k as i32) as i16, c as i16), lines[src][j].clone());
            }
        }
    } else {
        let lines: Vec<Vec<Cell>> = (col1..=col2)
            .map(|c| (row1..=row2).map(|r| cell_at(sheet, r, c)).collect())
            .collect();
        for (k, &src) in order.iter().enumerate() {
            for (j, r) in (row1..=row2).enumerate() {
                sheet
                    .data
                    .insert((r as i16, (col1 + k as i32) as i16), lines[src][j].clone());
            }
        }
    }
}

/// Every cell that (transitively) depends on `pos`, including through range
/// functions. Does not include `pos` itself.
pub fn transitive_dependents(sheet: &Sheet, pos: CellInfo) -> Vec<CellInfo> {
    let mut tree: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    let own_key = pos.col as i32 * 1000 + pos.row as i32;
    tree.insert(own_key, 0);
    add_to_tree(&mut tree, pos, sheet);
    let mut out: Vec<CellInfo> = tree
        .keys()
        .filter(|&&k| k != own_key)
        .map(|&k| CellInfo {
            row: (k % 1000) as i16,
            col: (k / 1000) as i16,
        })
        .collect();
    out.sort_by_key(|c| (c.row, c.col));
    out
}

/// Moves a cell from `from` to `to` with the desktop "cut + paste"
/// semantics (ported unchanged from the desktop `menu::paste`):
///
/// * the cell (value / string / formula) now lives at `to`; `from` becomes a
///   blank constant;
/// * formulas that referenced `from` are rewritten to reference `to`
///   (range formulas covering `from` are simply recalculated);
/// * formulas that referenced the *old* content of `to` are turned into
///   error constants (their source was overwritten);
/// * dependents are recalculated.
///
/// `from` and `to` must be different, in-bounds cells.
pub fn move_cell(sheet: &mut Sheet, from: CellInfo, to: CellInfo) {
    if from == to {
        return;
    }
    let (old_row, old_col) = (from.row, from.col);
    let (row, col) = (to.row, to.col);

    let old_cell1 = get_or_create_cell(sheet, old_row as i32, old_col as i32).clone();
    let old_cell2 = get_or_create_cell(sheet, row as i32, col as i32).clone();

    let mut new_cell1 = old_cell1.clone();
    new_cell1.value = 0;
    new_cell1.string = None;
    new_cell1.is_error = false;
    new_cell1.op_code = OpCode::NoConstraint;
    new_cell1.cell1 = CellInfo::NONE;
    new_cell1.cell2 = CellInfo::NONE;

    // The cells the moved formula references: re-register the dependency
    // under the new coordinates.
    let depended_cell1 = old_cell1.cell1.clone();
    let depended_cell2 = old_cell1.cell2.clone();
    let old_key = old_col as i32 * 1000 + old_row as i32;
    let new_key = col as i32 * 1000 + row as i32;
    if !depended_cell1.is_none() {
        let c = get_or_create_cell(sheet, depended_cell1.row as i32, depended_cell1.col as i32);
        c.dependencies.remove(&old_key);
        c.dependencies.insert(new_key);
    }
    if !depended_cell2.is_none() {
        let c = get_or_create_cell(sheet, depended_cell2.row as i32, depended_cell2.col as i32);
        c.dependencies.remove(&old_key);
        c.dependencies.insert(new_key);
    }

    remove_dependency(&to, sheet);

    *get_or_create_cell(sheet, old_row as i32, old_col as i32) = new_cell1.clone();
    change_dependecy_set(&mut new_cell1, sheet, false, row, col, old_row, old_col);
    let mut new_cell2 = old_cell1.clone();
    change_dependecy_set(&mut new_cell2, sheet, true, row, col, old_row, old_col);
    update_dependencies(old_row, old_col, row, col, sheet);

    // Formulas that referenced the overwritten target lose their source.
    for dependency in old_cell2.dependencies.iter() {
        let dependency_row = dependency % 1000;
        let dependency_col = dependency / 1000;
        let c = get_or_create_cell(sheet, dependency_row, dependency_col);
        c.value = 0;
        c.is_error = true;
        c.string = None;
        c.op_code = OpCode::NoConstraint;
        c.cell1 = CellInfo::NONE;
        c.cell2 = CellInfo::NONE;
        remove_dependency(
            &CellInfo {
                row: dependency_row as i16,
                col: dependency_col as i16,
            },
            sheet,
        );
    }

    *get_or_create_cell(sheet, row as i32, col as i32) = new_cell2;
    *get_or_create_cell(sheet, old_row as i32, old_col as i32) = new_cell1;
    recalculate_dependecy(from, sheet);
}

/// Copies the *displayed* content of `from` into `to` as a constant (the
/// desktop "copy + paste" semantics: formulas are not copied, their current
/// value / string / error state is). `to`'s previous formula is unregistered
/// and its dependents are recalculated. A never-written source clears `to`.
pub fn copy_cell_value(sheet: &mut Sheet, from: CellInfo, to: CellInfo) {
    if from == to {
        return;
    }
    let src = sheet.data.get(&(from.row, from.col)).cloned();
    remove_dependency(&to, sheet);
    let target = get_or_create_cell(sheet, to.row as i32, to.col as i32);
    match src {
        Some(src) => {
            target.value = src.value;
            target.string = src.string.clone();
            target.is_error = src.is_error;
            target.op_code = if src.string.is_some() {
                OpCode::String
            } else {
                OpCode::NoConstraint
            };
        }
        None => {
            target.value = 0;
            target.string = None;
            target.is_error = false;
            target.op_code = OpCode::NoConstraint;
        }
    }
    target.cell1 = CellInfo::NONE;
    target.cell2 = CellInfo::NONE;
    recalculate_dependecy(to, sheet);
}
