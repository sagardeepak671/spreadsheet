//! # spreadsheet-wasm
//!
//! Thin `wasm-bindgen` bridge around [`spreadsheet_core::Workbook`]. It contains
//! **no spreadsheet logic**: every method forwards to the core and only
//! converts types (`CoreError` -> JS exception, structs -> JSON strings).
//!
//! Conventions of the JS API:
//!
//! * Sheet indices are 0-based numbers; cell references are `"A1"` strings.
//! * Engine-level problems (`Invalid cmd`, `circular error`) are **returned**
//!   in an [`EditResult`] (`ok == false`) so the UI can show them inline.
//! * API misuse (bad sheet index, bad/out-of-range reference, bad JSON)
//!   **throws** a JS `Error` whose message comes from `CoreError`'s `Display`.
//! * Methods that return structured data (`snapshotJson`) return a JSON
//!   string; the TypeScript wrapper in `web/src/bridge.ts` parses it. This
//!   keeps the bridge free of `JsValue` so it also compiles and tests natively.
//!
//! The `#[wasm_bindgen]` attributes compile to plain Rust on native targets,
//! so `cargo test -p spreadsheet-wasm` exercises this exact code without a
//! browser; `wasm-pack test --node` runs the same tests inside wasm.

use spreadsheet_core::charts::ChartSpec;
use spreadsheet_core::workbook::{CoreError, FormatPatch, Workbook};
use wasm_bindgen::prelude::*;

/// Error type crossing the boundary. wasm-bindgen turns it into a JS `Error`
/// (via the `Into<JsValue>` impl) when a method returns `Err`. On native
/// targets it is just a plain Rust error, which is what lets the tests below
/// run with `cargo test` too (a `JsError` cannot be constructed natively).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeError(pub CoreError);

impl std::fmt::Display for BridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for BridgeError {}

impl From<CoreError> for BridgeError {
    fn from(e: CoreError) -> Self {
        BridgeError(e)
    }
}

impl From<BridgeError> for JsValue {
    fn from(e: BridgeError) -> JsValue {
        JsError::new(&e.to_string()).into()
    }
}

fn js_err(e: CoreError) -> BridgeError {
    BridgeError(e)
}

/// Result of a cell edit / command. Mirrors `spreadsheet_core::workbook::EditResult`.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct EditResult {
    ok: bool,
    status: String,
    sleep_seconds: i32,
}

#[wasm_bindgen]
impl EditResult {
    /// `true` when the engine reported `"ok"`.
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }

    /// Engine status text: `ok`, `Invalid cmd`, `circular error`, `err`.
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> String {
        self.status.clone()
    }

    /// Seconds a `SLEEP(..)` formula asked for. The engine never sleeps; the
    /// browser may `setTimeout` if it wants to mimic the CLI.
    #[wasm_bindgen(getter, js_name = sleepSeconds)]
    pub fn sleep_seconds(&self) -> i32 {
        self.sleep_seconds
    }
}

impl From<spreadsheet_core::workbook::EditResult> for EditResult {
    fn from(r: spreadsheet_core::workbook::EditResult) -> Self {
        EditResult {
            ok: r.ok,
            status: r.status,
            sleep_seconds: r.sleep_seconds,
        }
    }
}

/// A multi-sheet workbook living in wasm memory. Call `free()` when done
/// (or let FinalizationRegistry handle it in modern browsers).
#[wasm_bindgen]
pub struct WasmWorkbook {
    inner: Workbook,
}

#[wasm_bindgen]
impl WasmWorkbook {
    /// Creates a workbook with a single `"Sheet 1"` of `rows x cols`.
    #[wasm_bindgen(constructor)]
    pub fn new(rows: i32, cols: i32) -> Result<WasmWorkbook, BridgeError> {
        Ok(WasmWorkbook {
            inner: Workbook::with_default_sheet(rows, cols).map_err(js_err)?,
        })
    }

    /// Creates a workbook with no sheets (use `addSheet`).
    #[wasm_bindgen(js_name = empty)]
    pub fn empty() -> WasmWorkbook {
        WasmWorkbook {
            inner: Workbook::new(),
        }
    }

    /// Loads a workbook from the JSON produced by `toJson()` (same format as
    /// the desktop `.290` files).
    #[wasm_bindgen(js_name = fromJson)]
    pub fn from_json(json: &str) -> Result<WasmWorkbook, BridgeError> {
        Ok(WasmWorkbook {
            inner: Workbook::from_json(json).map_err(js_err)?,
        })
    }

    // ------------------------------------------------------------ sheets

    #[wasm_bindgen(js_name = sheetCount)]
    pub fn sheet_count(&self) -> usize {
        self.inner.sheet_count()
    }

    #[wasm_bindgen(js_name = sheetNames)]
    pub fn sheet_names(&self) -> Vec<String> {
        self.inner.sheet_names()
    }

    #[wasm_bindgen(js_name = sheetName)]
    pub fn sheet_name(&self, sheet: usize) -> Result<String, BridgeError> {
        Ok(self.inner.sheet(sheet).map_err(js_err)?.name.clone())
    }

    #[wasm_bindgen(js_name = sheetRows)]
    pub fn sheet_rows(&self, sheet: usize) -> Result<i32, BridgeError> {
        Ok(self.inner.sheet(sheet).map_err(js_err)?.sheet.rows)
    }

    #[wasm_bindgen(js_name = sheetCols)]
    pub fn sheet_cols(&self, sheet: usize) -> Result<i32, BridgeError> {
        Ok(self.inner.sheet(sheet).map_err(js_err)?.sheet.cols)
    }

    /// Appends an empty sheet; returns its index.
    #[wasm_bindgen(js_name = addSheet)]
    pub fn add_sheet(&mut self, name: &str, rows: i32, cols: i32) -> Result<usize, BridgeError> {
        self.inner.add_sheet(name, rows, cols).map_err(js_err)
    }

    /// Appends a sheet parsed from CSV text; returns its index.
    #[wasm_bindgen(js_name = addSheetFromCsv)]
    pub fn add_sheet_from_csv(&mut self, name: &str, csv_text: &str) -> usize {
        self.inner.add_sheet_from_csv(name, csv_text)
    }

    #[wasm_bindgen(js_name = removeSheet)]
    pub fn remove_sheet(&mut self, sheet: usize) -> Result<(), BridgeError> {
        self.inner.remove_sheet(sheet).map(|_| ()).map_err(js_err)
    }

    #[wasm_bindgen(js_name = renameSheet)]
    pub fn rename_sheet(&mut self, sheet: usize, name: &str) -> Result<(), BridgeError> {
        self.inner.rename_sheet(sheet, name).map_err(js_err)
    }

    // ------------------------------------------------------------ editing

    /// Sets cell `reference` (e.g. `"A1"`) to `input`, the right-hand side of
    /// a formula: `5`, `B1+5`, `SUM(A1:B2)`, `"text"`, `SLEEP(2)`.
    #[wasm_bindgen(js_name = setCell)]
    pub fn set_cell(
        &mut self,
        sheet: usize,
        reference: &str,
        input: &str,
    ) -> Result<EditResult, BridgeError> {
        self.inner
            .set_cell(sheet, reference, input)
            .map(Into::into)
            .map_err(js_err)
    }

    /// Stores `text` as a string cell (quotes are added for you).
    #[wasm_bindgen(js_name = setCellString)]
    pub fn set_cell_string(
        &mut self,
        sheet: usize,
        reference: &str,
        text: &str,
    ) -> Result<EditResult, BridgeError> {
        self.inner
            .set_cell_string(sheet, reference, text)
            .map(Into::into)
            .map_err(js_err)
    }

    /// Spreadsheet-style edit (what a grid editor should call): empty text
    /// clears the cell, a leading `=` means "strict formula", anything else
    /// is tried as a formula/number and otherwise stored as text.
    #[wasm_bindgen(js_name = editCell)]
    pub fn edit_cell(
        &mut self,
        sheet: usize,
        reference: &str,
        text: &str,
    ) -> Result<EditResult, BridgeError> {
        self.inner
            .edit_cell(sheet, reference, text)
            .map(Into::into)
            .map_err(js_err)
    }

    /// Resets a cell to "never written" and recalculates its dependents.
    #[wasm_bindgen(js_name = clearCell)]
    pub fn clear_cell(&mut self, sheet: usize, reference: &str) -> Result<EditResult, BridgeError> {
        self.inner
            .clear_cell(sheet, reference)
            .map(Into::into)
            .map_err(js_err)
    }

    /// Runs a raw command of the command language (`A1=B1+5`, `scroll_toC3`, …).
    #[wasm_bindgen(js_name = executeCommand)]
    pub fn execute_command(
        &mut self,
        sheet: usize,
        command: &str,
    ) -> Result<EditResult, BridgeError> {
        self.inner
            .execute_command(sheet, command)
            .map(Into::into)
            .map_err(js_err)
    }

    /// Clears every cell of the inclusive rectangle (one undo step).
    #[wasm_bindgen(js_name = clearRange)]
    pub fn clear_range(
        &mut self,
        sheet: usize,
        row1: i32,
        col1: i32,
        row2: i32,
        col2: i32,
    ) -> Result<EditResult, BridgeError> {
        self.inner
            .clear_range(sheet, (row1, col1, row2, col2))
            .map(Into::into)
            .map_err(js_err)
    }

    /// Pastes a JSON `string[][]` of editor texts with its top-left at
    /// `(row, col)` (one undo step). Used for text coming from the system
    /// clipboard.
    #[wasm_bindgen(js_name = pasteTextBlock)]
    pub fn paste_text_block(
        &mut self,
        sheet: usize,
        row: i32,
        col: i32,
        block_json: &str,
    ) -> Result<EditResult, BridgeError> {
        let block: Vec<Vec<String>> = serde_json::from_str(block_json)
            .map_err(|e| BridgeError(CoreError::Json(e.to_string())))?;
        self.inner
            .paste_text_block(sheet, (row, col), &block)
            .map(Into::into)
            .map_err(js_err)
    }

    /// Desktop "cut + paste": moves the rectangle so its top-left lands on
    /// `(to_row, to_col)`; formulas referencing moved cells follow them.
    #[wasm_bindgen(js_name = moveCells)]
    #[allow(clippy::too_many_arguments)]
    pub fn move_cells(
        &mut self,
        sheet: usize,
        row1: i32,
        col1: i32,
        row2: i32,
        col2: i32,
        to_row: i32,
        to_col: i32,
    ) -> Result<EditResult, BridgeError> {
        self.inner
            .move_cells(sheet, (row1, col1, row2, col2), (to_row, to_col))
            .map(Into::into)
            .map_err(js_err)
    }

    /// Desktop "copy + paste": pastes the current *values* of the rectangle
    /// (formulas are not copied).
    #[wasm_bindgen(js_name = copyValues)]
    #[allow(clippy::too_many_arguments)]
    pub fn copy_values(
        &mut self,
        sheet: usize,
        row1: i32,
        col1: i32,
        row2: i32,
        col2: i32,
        to_row: i32,
        to_col: i32,
    ) -> Result<EditResult, BridgeError> {
        self.inner
            .copy_values(sheet, (row1, col1, row2, col2), (to_row, to_col))
            .map(Into::into)
            .map_err(js_err)
    }

    /// JSON array of cell snapshots whose display/input text contains
    /// `needle` (`exact`: whole display text must match).
    #[wasm_bindgen(js_name = findJson)]
    pub fn find_json(
        &self,
        sheet: usize,
        needle: &str,
        exact: bool,
    ) -> Result<String, BridgeError> {
        let cells = self.inner.find(sheet, needle, exact).map_err(js_err)?;
        serde_json::to_string(&cells).map_err(|e| BridgeError(CoreError::Json(e.to_string())))
    }

    /// Desktop "Find and Replace" (exact match on constant cells). Returns
    /// JSON `{ matched, replaced }`.
    #[wasm_bindgen(js_name = findReplace)]
    pub fn find_replace(
        &mut self,
        sheet: usize,
        find: &str,
        replace: &str,
    ) -> Result<String, BridgeError> {
        let out = self
            .inner
            .find_and_replace(sheet, find, replace)
            .map_err(js_err)?;
        serde_json::to_string(&out).map_err(|e| BridgeError(CoreError::Json(e.to_string())))
    }

    /// Desktop "Sort": `key` is a column name (`"B"`, sorts rows) or a
    /// 1-based row number (`"3"`, sorts columns). Validation problems are
    /// reported in the result status.
    #[wasm_bindgen(js_name = sortRange)]
    #[allow(clippy::too_many_arguments)]
    pub fn sort_range(
        &mut self,
        sheet: usize,
        row1: i32,
        col1: i32,
        row2: i32,
        col2: i32,
        key: &str,
        ascending: bool,
    ) -> Result<EditResult, BridgeError> {
        self.inner
            .sort_range(sheet, (row1, col1, row2, col2), key, ascending)
            .map(Into::into)
            .map_err(js_err)
    }

    /// Runs a desktop `SORT(A1:B5;A;asc)` command; `undefined` if the text is
    /// not a sort command.
    #[wasm_bindgen(js_name = sortCommand)]
    pub fn sort_command(
        &mut self,
        sheet: usize,
        text: &str,
    ) -> Result<Option<EditResult>, BridgeError> {
        self.inner
            .sort_command(sheet, text)
            .map(|r| r.map(Into::into))
            .map_err(js_err)
    }

    // ------------------------------------------------------------ formatting

    /// JSON `CellFormat` of a cell (`{}` when unformatted).
    #[wasm_bindgen(js_name = getFormat)]
    pub fn get_format(&self, sheet: usize, reference: &str) -> Result<String, BridgeError> {
        let f = self.inner.cell_format(sheet, reference).map_err(js_err)?;
        serde_json::to_string(&f).map_err(|e| BridgeError(CoreError::Json(e.to_string())))
    }

    /// Applies a JSON `FormatPatch` (`{"bold":true,"fill":null,...}`; missing
    /// = leave, `null` = clear, `clearAll:true` = reset) to a rectangle. One
    /// undo step; values and formulas are untouched.
    #[wasm_bindgen(js_name = setFormat)]
    #[allow(clippy::too_many_arguments)]
    pub fn set_format(
        &mut self,
        sheet: usize,
        row1: i32,
        col1: i32,
        row2: i32,
        col2: i32,
        patch_json: &str,
    ) -> Result<EditResult, BridgeError> {
        let patch: FormatPatch = serde_json::from_str(patch_json)
            .map_err(|e| BridgeError(CoreError::Json(e.to_string())))?;
        self.inner
            .set_format(sheet, (row1, col1, row2, col2), &patch)
            .map(Into::into)
            .map_err(js_err)
    }

    // ------------------------------------------------------------ charts

    /// JSON array of the sheet's `ChartSpec`s.
    #[wasm_bindgen(js_name = chartsJson)]
    pub fn charts_json(&self, sheet: usize) -> Result<String, BridgeError> {
        let charts = self.inner.charts(sheet).map_err(js_err)?;
        serde_json::to_string(&charts).map_err(|e| BridgeError(CoreError::Json(e.to_string())))
    }

    /// Stores a chart (JSON `ChartSpec`, id is assigned) and returns its id.
    #[wasm_bindgen(js_name = addChart)]
    pub fn add_chart(&mut self, sheet: usize, spec_json: &str) -> Result<u32, BridgeError> {
        let spec: ChartSpec = serde_json::from_str(spec_json)
            .map_err(|e| BridgeError(CoreError::Json(e.to_string())))?;
        self.inner.add_chart(sheet, spec).map_err(js_err)
    }

    #[wasm_bindgen(js_name = updateChart)]
    pub fn update_chart(&mut self, sheet: usize, spec_json: &str) -> Result<(), BridgeError> {
        let spec: ChartSpec = serde_json::from_str(spec_json)
            .map_err(|e| BridgeError(CoreError::Json(e.to_string())))?;
        self.inner.update_chart(sheet, spec).map_err(js_err)
    }

    #[wasm_bindgen(js_name = removeChart)]
    pub fn remove_chart(&mut self, sheet: usize, id: u32) -> Result<(), BridgeError> {
        self.inner.remove_chart(sheet, id).map_err(js_err)
    }

    /// Live `ChartData` (JSON) for a `ChartSpec` (JSON), read from the cells.
    #[wasm_bindgen(js_name = chartDataJson)]
    pub fn chart_data_json(&self, sheet: usize, spec_json: &str) -> Result<String, BridgeError> {
        let spec: ChartSpec = serde_json::from_str(spec_json)
            .map_err(|e| BridgeError(CoreError::Json(e.to_string())))?;
        let data = self.inner.chart_data(sheet, &spec).map_err(js_err)?;
        serde_json::to_string(&data).map_err(|e| BridgeError(CoreError::Json(e.to_string())))
    }

    // ------------------------------------------------------------ history

    #[wasm_bindgen(js_name = canUndo)]
    pub fn can_undo(&self) -> bool {
        self.inner.can_undo()
    }

    #[wasm_bindgen(js_name = canRedo)]
    pub fn can_redo(&self) -> bool {
        self.inner.can_redo()
    }

    #[wasm_bindgen(js_name = clearHistory)]
    pub fn clear_history(&mut self) {
        self.inner.clear_history()
    }

    /// Reverts the last undoable operation. Returns JSON
    /// `{ label, sheet, references, result }` or `undefined` when there is
    /// nothing to undo.
    pub fn undo(&mut self) -> Result<Option<String>, BridgeError> {
        match self.inner.undo().map_err(js_err)? {
            Some(out) => {
                Ok(Some(serde_json::to_string(&out).map_err(|e| {
                    BridgeError(CoreError::Json(e.to_string()))
                })?))
            }
            None => Ok(None),
        }
    }

    pub fn redo(&mut self) -> Result<Option<String>, BridgeError> {
        match self.inner.redo().map_err(js_err)? {
            Some(out) => {
                Ok(Some(serde_json::to_string(&out).map_err(|e| {
                    BridgeError(CoreError::Json(e.to_string()))
                })?))
            }
            None => Ok(None),
        }
    }

    // ------------------------------------------------------------ reading

    /// Grid text for a cell: `"Err"`, the string, or the number.
    #[wasm_bindgen(js_name = getDisplayValue)]
    pub fn get_display_value(&self, sheet: usize, reference: &str) -> Result<String, BridgeError> {
        self.inner.display_value(sheet, reference).map_err(js_err)
    }

    /// Numeric value of a cell (0 for untouched / string / error cells).
    #[wasm_bindgen(js_name = getValue)]
    pub fn get_value(&self, sheet: usize, reference: &str) -> Result<i32, BridgeError> {
        self.inner.cell_value(sheet, reference).map_err(js_err)
    }

    #[wasm_bindgen(js_name = isError)]
    pub fn is_error(&self, sheet: usize, reference: &str) -> Result<bool, BridgeError> {
        self.inner.cell_is_error(sheet, reference).map_err(js_err)
    }

    /// Full command that recreates the cell, e.g. `"B2=A1+5"`.
    #[wasm_bindgen(js_name = getFormula)]
    pub fn get_formula(&self, sheet: usize, reference: &str) -> Result<String, BridgeError> {
        self.inner.formula(sheet, reference).map_err(js_err)
    }

    /// Editor text for a cell: `A1+5`, `"hello"`, `5` (`""` when untouched).
    #[wasm_bindgen(js_name = getInput)]
    pub fn get_input(&self, sheet: usize, reference: &str) -> Result<String, BridgeError> {
        self.inner.cell_input(sheet, reference).map_err(js_err)
    }

    /// JSON array of the non-empty cells inside `(row1,col1)..=(row2,col2)`
    /// (0-based, inclusive, clipped to the sheet). Same element shape as
    /// `snapshotJson().cells`. Used by the virtualized grid.
    #[wasm_bindgen(js_name = rangeJson)]
    pub fn range_json(
        &self,
        sheet: usize,
        row1: i32,
        col1: i32,
        row2: i32,
        col2: i32,
    ) -> Result<String, BridgeError> {
        let cells = self
            .inner
            .range_snapshot(sheet, row1, col1, row2, col2)
            .map_err(js_err)?;
        serde_json::to_string(&cells).map_err(|e| BridgeError(CoreError::Json(e.to_string())))
    }

    /// JSON: `{ name, rows, cols, cells: [{ row, col, reference, display,
    /// value, isError, isString, hasFormula, formula, input }] }` (non-empty
    /// cells only, row-major).
    #[wasm_bindgen(js_name = snapshotJson)]
    pub fn snapshot_json(&self, sheet: usize) -> Result<String, BridgeError> {
        let snap = self.inner.sheet_snapshot(sheet).map_err(js_err)?;
        serde_json::to_string(&snap).map_err(|e| BridgeError(CoreError::Json(e.to_string())))
    }

    // ------------------------------------------------------------ persistence

    /// Serialises all sheets (formulas + dependency graph included) as JSON.
    #[wasm_bindgen(js_name = toJson)]
    pub fn to_json(&self) -> Result<String, BridgeError> {
        self.inner.to_json().map_err(js_err)
    }

    /// Pretty-printed JSON, byte-compatible with what the desktop "Save"
    /// writes to `.290` files.
    #[wasm_bindgen(js_name = toJsonPretty)]
    pub fn to_json_pretty(&self) -> Result<String, BridgeError> {
        self.inner.to_json_pretty().map_err(js_err)
    }

    /// Current values of one sheet as CSV text.
    #[wasm_bindgen(js_name = toCsv)]
    pub fn to_csv(&self, sheet: usize) -> Result<String, BridgeError> {
        self.inner.sheet_to_csv(sheet).map_err(js_err)
    }
}

/// Crate version, handy for a "bridge is alive" check from JS.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ---------------------------------------------------------------------------
// Native tests: same code path as the browser, minus the JS glue.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_calculation_through_the_bridge() {
        let mut wb = WasmWorkbook::new(10, 10).unwrap();
        assert!(wb.set_cell(0, "A1", "10").unwrap().ok());
        assert!(wb.set_cell(0, "B1", "20").unwrap().ok());
        assert!(wb.set_cell(0, "C1", "A1+B1").unwrap().ok());
        assert!(wb.set_cell(0, "D1", "SUM(A1:C1)").unwrap().ok());
        assert_eq!(wb.get_display_value(0, "C1").unwrap(), "30");
        assert_eq!(wb.get_value(0, "D1").unwrap(), 60);
        wb.set_cell(0, "A1", "5").unwrap();
        assert_eq!(wb.get_display_value(0, "C1").unwrap(), "25");
        assert_eq!(wb.get_display_value(0, "D1").unwrap(), "50");
        assert_eq!(wb.get_formula(0, "D1").unwrap(), "D1=SUM(A1:C1)");
    }

    #[test]
    fn engine_errors_are_returned_api_errors_are_thrown() {
        let mut wb = WasmWorkbook::new(5, 5).unwrap();
        let r = wb.set_cell(0, "A1", "nonsense").unwrap();
        assert!(!r.ok());
        assert_eq!(r.status(), "Invalid cmd");

        wb.set_cell(0, "A1", "B1").unwrap();
        let r = wb.set_cell(0, "B1", "A1").unwrap();
        assert_eq!(r.status(), "circular error");

        assert_eq!(
            wb.set_cell(7, "A1", "1").unwrap_err(),
            BridgeError(CoreError::InvalidSheetIndex(7))
        );
        assert!(wb.set_cell(0, "Z9", "1").is_err());
        assert!(wb.get_display_value(0, "bad").is_err());
        assert!(WasmWorkbook::from_json("{").is_err());
        assert!(WasmWorkbook::new(0, 0).is_err());
    }

    #[test]
    fn sheets_json_and_csv() {
        let mut wb = WasmWorkbook::new(4, 4).unwrap();
        assert_eq!(wb.add_sheet("Two", 3, 3).unwrap(), 1);
        wb.rename_sheet(1, "Second").unwrap();
        assert_eq!(wb.sheet_names(), vec!["Sheet 1", "Second"]);
        wb.set_cell_string(1, "A1", "hello").unwrap();
        wb.set_cell(1, "B1", "A1").unwrap();
        assert_eq!(wb.get_display_value(1, "B1").unwrap(), "hello");

        let json = wb.to_json().unwrap();
        let back = WasmWorkbook::from_json(&json).unwrap();
        assert_eq!(back.sheet_count(), 2);
        assert_eq!(back.get_display_value(1, "B1").unwrap(), "hello");
        assert_eq!(back.to_csv(1).unwrap(), "hello,hello,0\n0,0,0\n0,0,0\n");

        let snap = back.snapshot_json(1).unwrap();
        assert!(snap.contains("\"reference\":\"A1\""));
        assert!(snap.contains("\"isString\":true"));

        wb.remove_sheet(0).unwrap();
        assert_eq!(wb.sheet_names(), vec!["Second"]);
        let idx = wb.add_sheet_from_csv("csv", "1,2\n3,4\n");
        assert_eq!(wb.sheet_rows(idx).unwrap(), 2);
        assert_eq!(wb.get_value(idx, "B2").unwrap(), 4);
    }

    #[test]
    fn grid_editing_helpers() {
        let mut wb = WasmWorkbook::new(5, 5).unwrap();
        assert!(wb.edit_cell(0, "A1", "plain text").unwrap().ok());
        assert_eq!(wb.get_input(0, "A1").unwrap(), "\"plain text\"");
        wb.edit_cell(0, "B1", "=3*2").unwrap();
        assert_eq!(wb.get_input(0, "B1").unwrap(), "6");
        wb.edit_cell(0, "C1", "B1+1").unwrap();
        assert_eq!(wb.get_input(0, "C1").unwrap(), "B1+1");
        let json = wb.range_json(0, 0, 0, 0, 1).unwrap();
        assert!(json.starts_with("[{"));
        assert!(json.contains("\"input\":\"6\""));
        assert!(!json.contains("C1"));
        assert!(wb.clear_cell(0, "C1").unwrap().ok());
        assert_eq!(wb.get_input(0, "C1").unwrap(), "");
        assert!(wb.edit_cell(0, "A1", "").unwrap().ok());
        assert_eq!(wb.get_display_value(0, "A1").unwrap(), "0");
        assert!(wb.range_json(0, 0, 0, 4, 4).unwrap().contains("B1"));
        assert!(wb.range_json(9, 0, 0, 1, 1).is_err());
    }

    #[test]
    fn workbook_editing_features_through_the_bridge() {
        let mut wb = WasmWorkbook::new(10, 10).unwrap();
        wb.edit_cell(0, "A1", "5").unwrap();
        wb.edit_cell(0, "B1", "=A1*2").unwrap();
        // cut/paste rewires, copy/paste keeps values
        wb.move_cells(0, 0, 0, 0, 0, 2, 2).unwrap();
        assert_eq!(wb.get_formula(0, "B1").unwrap(), "B1=C3*2");
        wb.copy_values(0, 0, 1, 0, 1, 4, 4).unwrap();
        assert_eq!(wb.get_input(0, "E5").unwrap(), "10");
        // find / replace / sort
        assert!(wb.find_json(0, "10", false).unwrap().contains("\"E5\""));
        let out = wb.find_replace(0, "10", "11").unwrap();
        assert!(out.contains("\"replaced\":1"));
        wb.paste_text_block(0, 5, 0, "[[\"3\"],[\"1\"],[\"2\"]]")
            .unwrap();
        assert!(wb.sort_range(0, 5, 0, 7, 0, "A", true).unwrap().ok());
        assert_eq!(wb.get_display_value(0, "A6").unwrap(), "1");
        assert!(
            wb.sort_command(0, "SORT(A6:A8;A;desc)")
                .unwrap()
                .unwrap()
                .ok()
        );
        assert!(wb.sort_command(0, "nope").unwrap().is_none());
        assert_eq!(wb.get_display_value(0, "A6").unwrap(), "3");
        assert!(wb.paste_text_block(0, 0, 0, "not json").is_err());
        // history
        assert!(wb.can_undo());
        let out = wb.undo().unwrap().unwrap();
        assert!(out.contains("\"label\":\"sort"));
        assert_eq!(wb.get_display_value(0, "A6").unwrap(), "1");
        assert!(wb.redo().unwrap().is_some());
        assert_eq!(wb.get_display_value(0, "A6").unwrap(), "3");
        wb.clear_range(0, 0, 0, 9, 9).unwrap();
        assert_eq!(wb.range_json(0, 0, 0, 9, 9).unwrap(), "[]");
        wb.clear_history();
        assert!(!wb.can_undo() && wb.undo().unwrap().is_none());
        assert!(wb.to_json_pretty().unwrap().contains('\n'));
    }

    #[test]
    fn formatting_and_charts_through_the_bridge() {
        let mut wb = WasmWorkbook::new(10, 10).unwrap();
        wb.edit_cell(0, "A1", "1").unwrap();
        wb.edit_cell(0, "A2", "2").unwrap();
        assert_eq!(wb.get_format(0, "A1").unwrap(), "{}");
        assert!(
            wb.set_format(0, 0, 0, 1, 0, r##"{"bold":true,"color":"#ff0000"}"##)
                .unwrap()
                .ok()
        );
        assert_eq!(
            wb.get_format(0, "A2").unwrap(),
            r##"{"bold":true,"color":"#ff0000"}"##
        );
        wb.set_format(0, 0, 0, 0, 0, r#"{"color":null}"#).unwrap();
        assert_eq!(wb.get_format(0, "A1").unwrap(), r#"{"bold":true}"#);
        assert!(wb.set_format(0, 0, 0, 0, 0, "nope").is_err());
        assert!(
            wb.range_json(0, 0, 0, 1, 0)
                .unwrap()
                .contains("\"format\":{\"bold\":true")
        );
        assert_eq!(wb.get_display_value(0, "A2").unwrap(), "2");

        let spec = r#"{"kind":"line","title":"t","range":{"row1":0,"col1":0,"row2":1,"col2":0}}"#;
        let id = wb.add_chart(0, spec).unwrap();
        assert_eq!(id, 1);
        let charts = wb.charts_json(0).unwrap();
        assert!(charts.contains("\"id\":1"));
        let data = wb.chart_data_json(0, spec).unwrap();
        assert!(data.contains("\"values\":[1.0,2.0]"));
        wb.update_chart(
            0,
            r#"{"id":1,"kind":"bar","range":{"row1":0,"col1":0,"row2":1,"col2":0}}"#,
        )
        .unwrap();
        assert!(wb.charts_json(0).unwrap().contains("\"kind\":\"bar\""));
        assert!(
            wb.update_chart(
                0,
                r#"{"id":9,"kind":"bar","range":{"row1":0,"col1":0,"row2":1,"col2":0}}"#
            )
            .is_err()
        );
        let json = wb.to_json().unwrap();
        let mut back = WasmWorkbook::from_json(&json).unwrap();
        assert_eq!(
            back.get_format(0, "A2").unwrap(),
            r##"{"bold":true,"color":"#ff0000"}"##
        );
        assert!(back.charts_json(0).unwrap().contains("\"id\":1"));
        back.remove_chart(0, 1).unwrap();
        assert_eq!(back.charts_json(0).unwrap(), "[]");
        assert!(back.remove_chart(0, 1).is_err());
    }

    #[test]
    fn sleep_is_reported_not_performed() {
        let mut wb = WasmWorkbook::new(5, 5).unwrap();
        let r = wb.set_cell(0, "A1", "SLEEP(3)").unwrap();
        assert!(r.ok());
        assert_eq!(r.sleep_seconds(), 3);
        assert_eq!(wb.get_value(0, "A1").unwrap(), 3);
        assert!(!version().is_empty());
    }
}
