//! Chart definitions stored in a sheet and the extraction of their data
//! from the cells.
//!
//! A `ChartSpec` only says *which* cells to plot and how; `chart_data`
//! reads the current cell values every time it is called, so a chart always
//! reflects the live spreadsheet state. Rendering (SVG/canvas) is the
//! front-end's job.

use crate::sheet_functions::{Sheet, col_num_to_col_name};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChartKind {
    Line,
    Bar,
    Scatter,
}

/// Inclusive 0-based rectangle of cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RangeRef {
    pub row1: i32,
    pub col1: i32,
    pub row2: i32,
    pub col2: i32,
}

impl RangeRef {
    pub fn normalized(self) -> RangeRef {
        RangeRef {
            row1: self.row1.min(self.row2),
            col1: self.col1.min(self.col2),
            row2: self.row1.max(self.row2),
            col2: self.col1.max(self.col2),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSpec {
    /// Unique within the sheet; assigned by `Workbook::add_chart`.
    #[serde(default)]
    pub id: u32,
    pub kind: ChartKind,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub x_label: String,
    #[serde(default)]
    pub y_label: String,
    pub range: RangeRef,
    /// `false`: every column of the range is a series; `true`: every row.
    #[serde(default)]
    pub series_in_rows: bool,
    /// The first cell of every series is its name.
    #[serde(default)]
    pub header_labels: bool,
    /// The first series supplies the X values (scatter / line) or the
    /// category labels (bar) instead of being plotted.
    #[serde(default)]
    pub first_series_is_x: bool,
}

/// One plotted series. `None` marks cells that cannot be plotted (errors,
/// text); never-written cells count as 0, like in the engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<Option<f64>>,
}

/// Everything a renderer needs, computed from the current cell values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartData {
    pub kind: ChartKind,
    pub title: String,
    pub x_label: String,
    pub y_label: String,
    /// Label for every data point (category names, X cell text, or 1..n).
    pub categories: Vec<String>,
    /// Numeric X values when `first_series_is_x` (else `None`).
    pub x: Option<Vec<Option<f64>>>,
    pub series: Vec<ChartSeries>,
    /// Human-readable problems (e.g. an empty range).
    pub warnings: Vec<String>,
}

/// Reads the cells described by `spec` from `sheet`.
pub fn chart_data(sheet: &Sheet, spec: &ChartSpec) -> ChartData {
    let range = spec.range.normalized();
    let r1 = range.row1.max(0);
    let c1 = range.col1.max(0);
    let r2 = range.row2.min(sheet.rows - 1);
    let c2 = range.col2.min(sheet.cols - 1);
    let mut warnings = Vec::new();

    let mut data = ChartData {
        kind: spec.kind,
        title: spec.title.clone(),
        x_label: spec.x_label.clone(),
        y_label: spec.y_label.clone(),
        categories: Vec::new(),
        x: None,
        series: Vec::new(),
        warnings: Vec::new(),
    };
    if r1 > r2 || c1 > c2 {
        data.warnings
            .push("the chart range is outside the sheet".to_string());
        return data;
    }

    // Lines = the raw series (before headers / x are split off).
    let lines: Vec<Vec<(i16, i16)>> = if spec.series_in_rows {
        (r1..=r2)
            .map(|r| (c1..=c2).map(|c| (r as i16, c as i16)).collect())
            .collect()
    } else {
        (c1..=c2)
            .map(|c| (r1..=r2).map(|r| (r as i16, c as i16)).collect())
            .collect()
    };
    let default_name = |k: usize| -> String {
        if spec.series_in_rows {
            format!("Row {}", r1 + k as i32 + 1)
        } else {
            col_num_to_col_name(c1 + k as i32)
        }
    };
    let text_of = |(r, c): (i16, i16)| -> String {
        sheet
            .data
            .get(&(r, c))
            .map(|cell| cell.display_value())
            .unwrap_or_default()
    };
    let number_of = |(r, c): (i16, i16)| -> Option<f64> {
        match sheet.data.get(&(r, c)) {
            None => Some(0.0),
            Some(cell) if cell.is_error || cell.string.is_some() => None,
            Some(cell) => Some(cell.value as f64),
        }
    };

    let mut raw: Vec<(String, Vec<(i16, i16)>)> = lines
        .into_iter()
        .enumerate()
        .map(|(k, mut cells)| {
            let name = if spec.header_labels && !cells.is_empty() {
                let head = cells.remove(0);
                let t = text_of(head);
                if t.is_empty() { default_name(k) } else { t }
            } else {
                default_name(k)
            };
            (name, cells)
        })
        .collect();

    let point_count = raw.first().map(|(_, c)| c.len()).unwrap_or(0);
    if point_count == 0 {
        warnings.push("the range has no data points (enable/disable 'first cell is the series name' or pick a bigger range)".to_string());
    }

    if spec.first_series_is_x && !raw.is_empty() {
        let (_, x_cells) = raw.remove(0);
        data.categories = x_cells.iter().map(|&c| text_of(c)).collect();
        data.x = Some(x_cells.iter().map(|&c| number_of(c)).collect());
        if raw.is_empty() {
            warnings.push(
                "only one series in the range; it was used as X, nothing is left to plot"
                    .to_string(),
            );
        }
    } else {
        data.categories = (1..=point_count).map(|i| i.to_string()).collect();
    }

    data.series = raw
        .into_iter()
        .map(|(name, cells)| ChartSeries {
            name,
            values: cells.iter().map(|&c| number_of(c)).collect(),
        })
        .collect();
    if data
        .series
        .iter()
        .all(|s| s.values.iter().all(|v| v.is_none()))
        && !data.series.is_empty()
    {
        warnings.push("no numeric values in the range".to_string());
    }
    data.warnings = warnings;
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sheet_functions::get_or_create_cell;

    fn sheet() -> Sheet {
        let mut s = Sheet::new(10, 10);
        // A: header + x, B/C: two numeric series, D: text
        for (r, (a, b, c)) in [
            ("x", "alpha", "beta"),
            ("1", "10", "5"),
            ("2", "20", "15"),
            ("3", "30", "25"),
        ]
        .iter()
        .enumerate()
        {
            for (col, text) in [(0, a), (1, b), (2, c)] {
                let cell = get_or_create_cell(&mut s, r as i32, col);
                match text.parse::<i32>() {
                    Ok(n) => cell.value = n,
                    Err(_) => {
                        cell.string = Some(text.to_string());
                        cell.op_code = crate::sheet_functions::OpCode::String;
                    }
                }
            }
        }
        get_or_create_cell(&mut s, 2, 1).is_error = true; // B3 = Err
        s
    }

    fn spec(kind: ChartKind) -> ChartSpec {
        ChartSpec {
            id: 0,
            kind,
            title: "t".into(),
            x_label: "x".into(),
            y_label: "y".into(),
            range: RangeRef {
                row1: 0,
                col1: 0,
                row2: 3,
                col2: 2,
            },
            series_in_rows: false,
            header_labels: true,
            first_series_is_x: true,
        }
    }

    #[test]
    fn columns_with_headers_and_x() {
        let data = chart_data(&sheet(), &spec(ChartKind::Line));
        assert_eq!(data.categories, vec!["1", "2", "3"]);
        assert_eq!(data.x, Some(vec![Some(1.0), Some(2.0), Some(3.0)]));
        assert_eq!(data.series.len(), 2);
        assert_eq!(data.series[0].name, "alpha");
        assert_eq!(data.series[0].values, vec![Some(10.0), None, Some(30.0)]); // Err -> None
        assert_eq!(data.series[1].name, "beta");
        assert_eq!(
            data.series[1].values,
            vec![Some(5.0), Some(15.0), Some(25.0)]
        );
        assert!(data.warnings.is_empty());
    }

    #[test]
    fn rows_without_headers_and_blank_cells() {
        let mut s = spec(ChartKind::Bar);
        s.series_in_rows = true;
        s.header_labels = false;
        s.first_series_is_x = false;
        s.range = RangeRef {
            row1: 1,
            col1: 1,
            row2: 2,
            col2: 4,
        }; // B2:E3 (E is blank)
        let data = chart_data(&sheet(), &s);
        assert_eq!(data.categories, vec!["1", "2", "3", "4"]);
        assert_eq!(data.x, None);
        assert_eq!(data.series[0].name, "Row 2");
        assert_eq!(
            data.series[0].values,
            vec![Some(10.0), Some(5.0), Some(0.0), Some(0.0)]
        ); // blank = 0
        assert_eq!(
            data.series[1].values,
            vec![None, Some(15.0), Some(0.0), Some(0.0)]
        );
    }

    #[test]
    fn reversed_and_out_of_range_specs() {
        let mut s = spec(ChartKind::Scatter);
        s.range = RangeRef {
            row1: 3,
            col1: 2,
            row2: 0,
            col2: 0,
        };
        assert_eq!(chart_data(&sheet(), &s).series.len(), 2);
        s.range = RangeRef {
            row1: 50,
            col1: 50,
            row2: 60,
            col2: 60,
        };
        let data = chart_data(&sheet(), &s);
        assert!(data.series.is_empty());
        assert!(!data.warnings.is_empty());
        s.range = RangeRef {
            row1: 0,
            col1: 0,
            row2: 3,
            col2: 0,
        }; // only the x column
        let data = chart_data(&sheet(), &s);
        assert!(data.series.is_empty());
        assert!(data.warnings.iter().any(|w| w.contains("only one series")));
    }

    #[test]
    fn specs_round_trip_through_json_with_defaults() {
        let json = r#"{"kind":"bar","range":{"row1":0,"col1":0,"row2":2,"col2":1}}"#;
        let s: ChartSpec = serde_json::from_str(json).unwrap();
        assert_eq!(s.id, 0);
        assert!(!s.header_labels && !s.first_series_is_x && !s.series_in_rows);
        let back: ChartSpec = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
