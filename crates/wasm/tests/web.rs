//! Bridge tests that run *inside* wasm (Node or a headless browser):
//!
//! ```sh
//! wasm-pack test --node crates/wasm
//! ```
//!
//! They mirror the native unit tests in `src/lib.rs` but additionally prove
//! that the whole engine (regex parser, HashMap-based dependency graph,
//! serde_json, LazyLock statics) behaves identically when compiled to
//! `wasm32-unknown-unknown`.

#![cfg(target_arch = "wasm32")]

use spreadsheet_wasm::WasmWorkbook;
use wasm_bindgen_test::*;

#[wasm_bindgen_test]
fn calculation_and_cascade() {
    let mut wb = WasmWorkbook::new(10, 10).unwrap();
    wb.set_cell(0, "A1", "10").unwrap();
    wb.set_cell(0, "B1", "20").unwrap();
    wb.set_cell(0, "C1", "A1+B1").unwrap();
    wb.set_cell(0, "D1", "SUM(A1:C1)").unwrap();
    wb.set_cell(0, "E1", "MAX(A1:D1)").unwrap();
    wb.set_cell(0, "F1", "D1/3").unwrap();
    assert_eq!(wb.get_display_value(0, "C1").unwrap(), "30");
    assert_eq!(wb.get_display_value(0, "D1").unwrap(), "60");
    assert_eq!(wb.get_display_value(0, "E1").unwrap(), "60");
    assert_eq!(wb.get_display_value(0, "F1").unwrap(), "20");

    wb.set_cell(0, "A1", "1").unwrap();
    assert_eq!(wb.get_display_value(0, "C1").unwrap(), "21");
    assert_eq!(wb.get_display_value(0, "D1").unwrap(), "42");
    assert_eq!(wb.get_display_value(0, "E1").unwrap(), "42");
    assert_eq!(wb.get_display_value(0, "F1").unwrap(), "14");
}

#[wasm_bindgen_test]
fn errors_strings_and_status() {
    let mut wb = WasmWorkbook::new(5, 5).unwrap();
    wb.set_cell(0, "A1", "0").unwrap();
    let r = wb.set_cell(0, "B1", "1/A1").unwrap();
    assert!(r.ok());
    assert!(wb.is_error(0, "B1").unwrap());
    assert_eq!(wb.get_display_value(0, "B1").unwrap(), "Err");

    wb.set_cell_string(0, "C1", "text").unwrap();
    assert_eq!(wb.get_display_value(0, "C1").unwrap(), "text");
    assert_eq!(wb.get_formula(0, "C1").unwrap(), "C1=text");

    wb.set_cell(0, "D1", "E1").unwrap();
    let r = wb.set_cell(0, "E1", "D1").unwrap();
    assert!(!r.ok());
    assert_eq!(r.status(), "circular error");

    let r = wb.set_cell(0, "A1", "SLEEP(1)").unwrap();
    assert_eq!(r.sleep_seconds(), 1);
}

#[wasm_bindgen_test]
fn api_errors_become_results() {
    let mut wb = WasmWorkbook::new(3, 3).unwrap();
    assert!(wb.set_cell(1, "A1", "1").is_err());
    assert!(wb.set_cell(0, "D4", "1").is_err());
    assert!(wb.get_value(0, "x").is_err());
    assert!(WasmWorkbook::from_json("[").is_err());
}

#[wasm_bindgen_test]
fn sheets_persistence_and_csv() {
    let mut wb = WasmWorkbook::new(3, 3).unwrap();
    wb.add_sheet("Second", 2, 2).unwrap();
    wb.set_cell(1, "A1", "7").unwrap();
    wb.set_cell(1, "B2", "A1*2").unwrap();
    let json = wb.to_json().unwrap();
    let mut back = WasmWorkbook::from_json(&json).unwrap();
    assert_eq!(back.sheet_names(), vec!["Sheet 1", "Second"]);
    assert_eq!(back.get_value(1, "B2").unwrap(), 14);
    back.set_cell(1, "A1", "1").unwrap();
    assert_eq!(back.get_value(1, "B2").unwrap(), 2);
    assert_eq!(back.to_csv(1).unwrap(), "1,0\n0,2\n");

    let snap = back.snapshot_json(1).unwrap();
    assert!(snap.contains("\"formula\":\"B2=A1*2\""));

    back.remove_sheet(0).unwrap();
    assert_eq!(back.sheet_count(), 1);
    let idx = back.add_sheet_from_csv("csv", "1,2\n3,4\n");
    assert_eq!(back.get_value(idx, "B2").unwrap(), 4);
}

#[wasm_bindgen_test]
fn grid_helpers_in_wasm() {
    let mut wb = WasmWorkbook::new(20, 20).unwrap();
    wb.edit_cell(0, "A1", "hello world").unwrap();
    wb.edit_cell(0, "B1", "=A1").unwrap();
    assert_eq!(wb.get_display_value(0, "B1").unwrap(), "hello world");
    assert_eq!(wb.get_input(0, "A1").unwrap(), "\"hello world\"");
    wb.edit_cell(0, "C3", "41+1").unwrap();
    let json = wb.range_json(0, 0, 0, 5, 5).unwrap();
    assert!(json.contains("\"reference\":\"C3\""));
    assert!(json.contains("\"display\":\"42\""));
    wb.clear_cell(0, "A1").unwrap();
    assert_eq!(wb.get_display_value(0, "B1").unwrap(), "0");
}

#[wasm_bindgen_test]
fn workbook_features_in_wasm() {
    let mut wb = WasmWorkbook::new(10, 10).unwrap();
    wb.edit_cell(0, "A1", "5").unwrap();
    wb.edit_cell(0, "B1", "=A1*2").unwrap();
    wb.move_cells(0, 0, 0, 0, 0, 2, 2).unwrap();
    assert_eq!(wb.get_formula(0, "B1").unwrap(), "B1=C3*2");
    assert!(wb.undo().unwrap().is_some());
    assert_eq!(wb.get_formula(0, "B1").unwrap(), "B1=A1*2");
    assert!(
        wb.find_replace(0, "5", "6")
            .unwrap()
            .contains("\"replaced\":1")
    );
    assert_eq!(wb.get_display_value(0, "B1").unwrap(), "12");
    let json = wb.to_json_pretty().unwrap();
    let back = WasmWorkbook::from_json(&json).unwrap();
    assert_eq!(back.get_display_value(0, "B1").unwrap(), "12");
}

#[wasm_bindgen_test]
fn formatting_and_charts_in_wasm() {
    let mut wb = WasmWorkbook::new(5, 5).unwrap();
    wb.edit_cell(0, "A1", "3").unwrap();
    wb.set_format(0, 0, 0, 0, 0, r#"{"italic":true,"align":"center"}"#)
        .unwrap();
    assert_eq!(
        wb.get_format(0, "A1").unwrap(),
        r#"{"italic":true,"align":"center"}"#
    );
    wb.add_chart(
        0,
        r#"{"kind":"scatter","range":{"row1":0,"col1":0,"row2":0,"col2":0}}"#,
    )
    .unwrap();
    let back = WasmWorkbook::from_json(&wb.to_json().unwrap()).unwrap();
    assert!(back.charts_json(0).unwrap().contains("scatter"));
    assert!(back.get_format(0, "A1").unwrap().contains("italic"));
    assert_eq!(back.get_display_value(0, "A1").unwrap(), "3");
}
