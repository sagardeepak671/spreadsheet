//! Reconstructs the formula text (`A1=B1+5`, `C3=SUM(A1:B2)`, …) of a cell
//! from its stored `OpCode` / references. This is the inverse of the parser
//! and is what the desktop formula bar and the wasm `getFormula` show.
//!
//! Moved unchanged from the desktop `ui/utils.rs` so the browser front-end can
//! share it.

use crate::sheet_functions::Cell;
use crate::sheet_functions::OpCode;
use crate::sheet_functions::col_num_to_col_name;

/// Returns the full command that would recreate `cell` at `(row, col)`,
/// e.g. `"B2=A1+5"`. Constant cells yield `"B2=5"`, string cells `"B2=text"`.
pub fn get_cell_formula(row: i16, col: i16, cell: &Cell) -> String {
    match cell.op_code {
        OpCode::NoConstraint => format!(
            "{}{}={}",
            col_num_to_col_name(col as i32),
            row + 1,
            cell.value
        ),
        OpCode::CellEqualsCell => format!(
            "{}{}={}{}",
            col_num_to_col_name(col as i32),
            row + 1,
            col_num_to_col_name(cell.cell1.col as i32),
            cell.cell1.row + 1
        ),
        OpCode::CellPlusCell
        | OpCode::CellMinusCell
        | OpCode::CellTimesCell
        | OpCode::CellDivideCell => {
            let operator = match cell.op_code {
                OpCode::CellPlusCell => "+",
                OpCode::CellMinusCell => "-",
                OpCode::CellTimesCell => "*",
                OpCode::CellDivideCell => "/",
                _ => unreachable!(),
            };
            format!(
                "{}{}={}{}{}{}{}",
                col_num_to_col_name(col as i32),
                row + 1,
                col_num_to_col_name(cell.cell1.col as i32),
                cell.cell1.row + 1,
                operator,
                col_num_to_col_name(cell.cell2.col as i32),
                cell.cell2.row + 1
            )
        }
        OpCode::CellPlusConstant
        | OpCode::CellMinusConstant
        | OpCode::CellTimesConstant
        | OpCode::CellDivideConstant
        | OpCode::ConstantMinusCell => {
            let operator = match cell.op_code {
                OpCode::CellPlusConstant => "+",
                OpCode::CellMinusConstant => "-",
                OpCode::ConstantMinusCell => "-",
                OpCode::CellTimesConstant => "*",
                OpCode::CellDivideConstant => "/",
                _ => unreachable!(),
            };
            let constant = constant_from_cell2(cell);
            if cell.op_code == OpCode::ConstantMinusCell {
                // `5-A1` is stored as ConstantMinusCell; keep the operand order
                // the user typed so the formula round-trips through the parser.
                format!(
                    "{}{}={}{}{}{}",
                    col_num_to_col_name(col as i32),
                    row + 1,
                    constant,
                    operator,
                    col_num_to_col_name(cell.cell1.col as i32),
                    cell.cell1.row + 1
                )
            } else {
                format!(
                    "{}{}={}{}{}{}",
                    col_num_to_col_name(col as i32),
                    row + 1,
                    col_num_to_col_name(cell.cell1.col as i32),
                    cell.cell1.row + 1,
                    operator,
                    constant
                )
            }
        }
        OpCode::ConstantDividesCell => {
            let constant = constant_from_cell2(cell);
            format!(
                "{}{}={}/{}{}",
                col_num_to_col_name(col as i32),
                row + 1,
                constant,
                col_num_to_col_name(cell.cell1.col as i32),
                cell.cell1.row + 1
            )
        }
        OpCode::Sum | OpCode::Min | OpCode::Max | OpCode::Avg | OpCode::Stdev => {
            let func_name = match cell.op_code {
                OpCode::Sum => "SUM",
                OpCode::Min => "MIN",
                OpCode::Max => "MAX",
                OpCode::Avg => "AVG",
                OpCode::Stdev => "STDEV",
                _ => unreachable!(),
            };
            format!(
                "{}{}={}({}{}:{}{})",
                col_num_to_col_name(col as i32),
                row + 1,
                func_name,
                col_num_to_col_name(cell.cell1.col as i32),
                cell.cell1.row + 1,
                col_num_to_col_name(cell.cell2.col as i32),
                cell.cell2.row + 1
            )
        }
        OpCode::Sleep => format!(
            "{}{}=SLEEP({}{})",
            col_num_to_col_name(col as i32),
            row + 1,
            col_num_to_col_name(cell.cell1.col as i32),
            cell.cell1.row + 1,
        ),
        OpCode::String => format!(
            "{}{}={}",
            col_num_to_col_name(col as i32),
            row + 1,
            cell.string.clone().unwrap_or_else(|| "None".to_string())
        ),
    }
}

/// The text a user would type to recreate `cell`: the right-hand side of
/// [`get_cell_formula`], except that string cells keep their quotes
/// (`"hello"`) so the result round-trips through the parser unchanged.
///
/// This is what an editor / formula bar should show.
pub fn get_cell_input(row: i16, col: i16, cell: &Cell) -> String {
    if cell.op_code == OpCode::String {
        return format!("\"{}\"", cell.string.clone().unwrap_or_default());
    }
    let full = get_cell_formula(row, col, cell);
    match full.find('=') {
        Some(i) => full[i + 1..].to_string(),
        None => full,
    }
}

/// `cell op constant` formulas smuggle the 32-bit constant through `cell2`
/// (`row` = high 16 bits, `col` = low 16 bits). Recover it.
fn constant_from_cell2(cell: &Cell) -> i32 {
    ((cell.cell2.row as i32) << 16) | (cell.cell2.col as i32 & 0xFFFF)
}
