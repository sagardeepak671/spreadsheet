//! CSV text <-> `Sheet` conversion (values only; formulas are exported as
//! their computed value, exactly like the original desktop "Save to CSV").
//!
//! No file I/O here: the desktop wraps these with `std::fs`, the browser hands
//! the text to/from JavaScript.

use crate::sheet_functions::{Cell, OpCode, Sheet};

/// Renders every cell of the sheet (including untouched ones, as `0`) as CSV.
/// Error cells become `Err`, strings containing a comma are double-quoted.
pub fn sheet_to_csv(sheet: &Sheet) -> String {
    let mut out = String::new();
    for row in 0..sheet.rows {
        let row_values: Vec<String> = (0..sheet.cols)
            .map(|col| match sheet.data.get(&(row as i16, col as i16)) {
                Some(cell) if cell.is_error => "Err".to_string(),
                Some(cell) => match &cell.string {
                    Some(s) if s.contains(',') => format!("\"{}\"", s),
                    Some(s) => s.clone(),
                    None => cell.value.to_string(),
                },
                None => "0".to_string(),
            })
            .collect();
        out.push_str(&row_values.join(","));
        out.push('\n');
    }
    out
}

/// Parses CSV text into a fresh sheet sized to the data (at least 1x1).
///
/// * quoted fields and unparsable fields become string cells
/// * integer fields become constant cells
/// * empty / `0` fields are left untouched (implicitly 0)
///
/// Fields are split on commas; a field wrapped in double quotes may contain
/// commas (this is what [`sheet_to_csv`] emits for such strings).
pub fn sheet_from_csv(text: &str) -> Sheet {
    let mut cells: Vec<((i16, i16), Cell)> = Vec::new();
    let mut max_row: i32 = -1;
    let mut max_col: i32 = -1;

    for (row_idx, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        for (col_idx, value) in split_csv_line(line).into_iter().enumerate() {
            max_row = max_row.max(row_idx as i32);
            max_col = max_col.max(col_idx as i32);
            let trimmed = value.as_str().trim();
            if trimmed.is_empty() {
                continue;
            }
            let mut cell = Cell::empty();
            if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
                cell.string = Some(trimmed[1..trimmed.len() - 1].to_string());
                cell.op_code = OpCode::String;
            } else {
                match trimmed.parse::<i32>() {
                    Ok(num) => cell.value = num,
                    Err(_) => {
                        cell.string = Some(trimmed.to_string());
                        cell.op_code = OpCode::String;
                    }
                }
            }
            if cell.value != 0 || cell.string.is_some() {
                cells.push(((row_idx as i16, col_idx as i16), cell));
            }
        }
    }

    let rows = (max_row + 1).max(1);
    let cols = (max_col + 1).max(1);
    let mut sheet = Sheet::new(rows, cols);
    for (key, cell) in cells {
        sheet.data.insert(key, cell);
    }
    sheet
}

/// Splits one CSV line on commas, keeping quoted fields (including their
/// quotes) intact.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in line.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            }
            ',' if !in_quotes => fields.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    fields.push(current);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_functions::get_or_create_cell;

    #[test]
    fn round_trip_values_and_strings() {
        let mut sheet = Sheet::new(2, 3);
        get_or_create_cell(&mut sheet, 0, 0).value = 7;
        let c = get_or_create_cell(&mut sheet, 1, 2);
        c.string = Some("hi, there".into());
        c.op_code = OpCode::String;
        let text = sheet_to_csv(&sheet);
        assert_eq!(text, "7,0,0\n0,0,\"hi, there\"\n");

        let back = sheet_from_csv(&text);
        assert_eq!((back.rows, back.cols), (2, 3));
        assert_eq!(back.data.get(&(0, 0)).unwrap().value, 7);
        assert_eq!(
            back.data.get(&(1, 2)).unwrap().string.as_deref(),
            Some("hi, there")
        );
        assert!(!back.data.contains_key(&(0, 1)));
    }

    #[test]
    fn quoted_fields_keep_their_commas() {
        assert_eq!(split_csv_line("1,\"a, b\",c"), vec!["1", "\"a, b\"", "c"]);
        assert_eq!(split_csv_line(""), vec![""]);
    }

    #[test]
    fn sheet_is_sized_to_data_so_range_functions_do_not_panic() {
        let sheet = sheet_from_csv("1,2,3\n4,5,6\n");
        assert_eq!(sheet.buul.len(), 2);
        assert_eq!(sheet.buul[0].len(), 3);
    }
}
