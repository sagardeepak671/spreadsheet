/**
 * Bridge tests: TypeScript → wasm → Rust core, executed by vitest in Node.
 *
 * Requires the wasm package to be built first (`npm run build:wasm`). The
 * `.wasm` is loaded from disk synchronously because Node's `fetch` does not
 * support `file:` URLs.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";

import {
  bridgeVersion,
  cellRef,
  loadSpreadsheetWasmSync,
  SpreadsheetWorkbook,
} from "./bridge";

beforeAll(() => {
  const wasmPath = fileURLToPath(new URL("./wasm-pkg/spreadsheet_wasm_bg.wasm", import.meta.url));
  loadSpreadsheetWasmSync(readFileSync(wasmPath));
});

describe("wasm bridge", () => {
  it("loads and reports a version", () => {
    expect(bridgeVersion()).toMatch(/^\d+\.\d+\.\d+$/);
  });

  it("creates a workbook with a default sheet", () => {
    const wb = SpreadsheetWorkbook.create(10, 8);
    expect(wb.sheetCount).toBe(1);
    expect(wb.sheetNames()).toEqual(["Sheet 1"]);
    expect(wb.sheetSize(0)).toEqual({ rows: 10, cols: 8 });
    wb.free();
  });

  it("calculates formulas in Rust and recalculates dependents", () => {
    const wb = SpreadsheetWorkbook.create(10, 10);
    expect(wb.setCell(0, "A1", "10").ok).toBe(true);
    expect(wb.setCell(0, "B1", "20").ok).toBe(true);
    expect(wb.setCell(0, "C1", "A1+B1").ok).toBe(true);
    expect(wb.setCell(0, "D1", "SUM(A1:C1)").ok).toBe(true);
    expect(wb.setCell(0, "E1", "D1/4").ok).toBe(true);
    expect(wb.setCell(0, "F1", "AVG(A1:D1)").ok).toBe(true);

    expect(wb.getDisplayValue(0, "C1")).toBe("30");
    expect(wb.getValue(0, "D1")).toBe(60);
    expect(wb.getValue(0, "E1")).toBe(15);
    expect(wb.getValue(0, "F1")).toBe(30);

    // editing A1 cascades through C1 -> D1 -> E1 / F1
    wb.setCell(0, "A1", "2");
    expect(wb.getValue(0, "C1")).toBe(22);
    expect(wb.getValue(0, "D1")).toBe(44);
    expect(wb.getValue(0, "E1")).toBe(11);
    expect(wb.getValue(0, "F1")).toBe(22);

    expect(wb.getFormula(0, "D1")).toBe("D1=SUM(A1:C1)");
    expect(wb.getFormula(0, "J10")).toBe("J10=0");
    expect(wb.getDisplayValue(0, "J10")).toBe("0");
    wb.free();
  });

  it("handles strings, errors and circular references like the desktop engine", () => {
    const wb = SpreadsheetWorkbook.create(5, 5);
    expect(wb.setCellString(0, "A1", "hello").ok).toBe(true);
    expect(wb.getDisplayValue(0, "A1")).toBe("hello");
    expect(wb.setCell(0, "B1", 'A1').ok).toBe(true);
    expect(wb.getDisplayValue(0, "B1")).toBe("hello");

    wb.setCell(0, "C1", "0");
    wb.setCell(0, "D1", "7/C1");
    expect(wb.isError(0, "D1")).toBe(true);
    expect(wb.getDisplayValue(0, "D1")).toBe("Err");
    wb.setCell(0, "C1", "7");
    expect(wb.getDisplayValue(0, "D1")).toBe("1");

    wb.setCell(0, "A2", "B2");
    const circular = wb.setCell(0, "B2", "A2");
    expect(circular.ok).toBe(false);
    expect(circular.status).toBe("circular error");

    const invalid = wb.setCell(0, "A3", "what is this");
    expect(invalid.ok).toBe(false);
    expect(invalid.status).toBe("Invalid cmd");

    const sleep = wb.setCell(0, "A4", "SLEEP(2)");
    expect(sleep.ok).toBe(true);
    expect(sleep.sleepSeconds).toBe(2);
    wb.free();
  });

  it("throws JS errors for API misuse", () => {
    const wb = SpreadsheetWorkbook.create(3, 3);
    expect(() => wb.setCell(4, "A1", "1")).toThrow(/no sheet at index 4/);
    expect(() => wb.setCell(0, "Z9", "1")).toThrow(/outside the sheet/);
    expect(() => wb.getDisplayValue(0, "nope")).toThrow(/invalid cell reference/);
    expect(() => SpreadsheetWorkbook.fromJson("{ not json")).toThrow(/json error/);
    expect(() => SpreadsheetWorkbook.create(0, 0)).toThrow(/invalid sheet size/);
    wb.free();
  });

  it("supports sheet operations", () => {
    const wb = SpreadsheetWorkbook.create(4, 4);
    expect(wb.addSheet("Data", 5, 5)).toBe(1);
    wb.renameSheet(1, "Renamed");
    expect(wb.sheetNames()).toEqual(["Sheet 1", "Renamed"]);
    wb.setCell(1, "A1", "42");
    expect(wb.getValue(1, "A1")).toBe(42);
    expect(wb.getValue(0, "A1")).toBe(0);
    wb.removeSheet(0);
    expect(wb.sheetNames()).toEqual(["Renamed"]);
    expect(wb.getValue(0, "A1")).toBe(42);
    expect(() => wb.removeSheet(3)).toThrow();
    wb.free();
  });

  it("round-trips through JSON with live formulas and exports CSV", () => {
    const wb = SpreadsheetWorkbook.create(3, 3);
    wb.setCell(0, "A1", "5");
    wb.setCell(0, "B1", "A1*2");
    wb.setCellString(0, "C1", "x");
    const json = wb.toJson();
    wb.free();

    const back = SpreadsheetWorkbook.fromJson(json);
    expect(back.getValue(0, "B1")).toBe(10);
    back.setCell(0, "A1", "6");
    expect(back.getValue(0, "B1")).toBe(12);
    expect(back.toCsv(0)).toBe("6,12,x\n0,0,0\n0,0,0\n");

    const idx = back.addSheetFromCsv("imported", "1,2\n3,four\n");
    expect(back.sheetSize(idx)).toEqual({ rows: 2, cols: 2 });
    expect(back.getDisplayValue(idx, "B2")).toBe("four");
    back.free();
  });

  it("provides a typed snapshot for rendering", () => {
    const wb = SpreadsheetWorkbook.create(3, 3);
    wb.setCell(0, "B2", "1");
    wb.setCell(0, "A1", "B2+1");
    const snap = wb.snapshot(0);
    expect(snap).toMatchObject({ name: "Sheet 1", rows: 3, cols: 3 });
    expect(snap.cells.map((c) => c.reference)).toEqual(["A1", "B2"]);
    expect(snap.cells[0]).toMatchObject({
      row: 0,
      col: 0,
      display: "2",
      value: 2,
      isError: false,
      isString: false,
      hasFormula: true,
      formula: "A1=B2+1",
    });
    wb.free();
  });

  it("editCell applies the spreadsheet editing policy in Rust", () => {
    const wb = SpreadsheetWorkbook.create(5, 5);
    expect(wb.editCell(0, "A1", "hello world").ok).toBe(true);
    expect(wb.getDisplayValue(0, "A1")).toBe("hello world");
    expect(wb.getInput(0, "A1")).toBe('"hello world"');
    wb.editCell(0, "B1", "5");
    wb.editCell(0, "C1", "=B1*3");
    expect(wb.getInput(0, "C1")).toBe("B1*3");
    expect(wb.getValue(0, "C1")).toBe(15);
    const broken = wb.editCell(0, "D1", "=SUM(B1:C1");
    expect(broken.ok).toBe(false);
    expect(wb.getInput(0, "D1")).toBe("");
    expect(wb.editCell(0, "B1", "").ok).toBe(true); // empty clears
    expect(wb.getValue(0, "C1")).toBe(0);
    expect(wb.clearCell(0, "C1").ok).toBe(true);
    expect(wb.getInput(0, "C1")).toBe("");
    wb.free();
  });

  it("range() returns only the non-empty cells of a window", () => {
    const wb = SpreadsheetWorkbook.create(1000, 1000);
    wb.editCell(0, "A1", "1");
    wb.editCell(0, "C3", "A1+1");
    wb.editCell(0, "ALL1000", "42");
    expect(wb.range(0, 0, 0, 30, 15).map((c) => c.reference)).toEqual(["A1", "C3"]);
    const far = wb.range(0, 990, 990, 999, 999);
    expect(far).toHaveLength(1);
    expect(far[0]).toMatchObject({ reference: "ALL1000", display: "42", input: "42", row: 999, col: 999 });
    expect(wb.range(0, 500, 500, 520, 520)).toEqual([]);
    wb.free();
  });

  it("save / load round trip keeps multiple sheets, formulas and dependencies", () => {
    const wb = SpreadsheetWorkbook.create(1000, 1000);
    wb.addSheet("Data", 50, 10);
    wb.addSheet("Summary", 5, 5);
    wb.renameSheet(0, "Main");
    wb.editCell(0, "A1", "10");
    wb.editCell(0, "B1", "=A1*3");
    wb.editCell(0, "C1", "=SUM(A1:B1)");
    wb.editCell(0, "D1", "hello world");
    wb.editCell(1, "A1", "=1/0");
    wb.editCell(2, "A1", "=SLEEP(2)");
    const pretty = wb.toJsonPretty();
    const compact = wb.toJson();
    expect(pretty.length).toBeGreaterThan(compact.length);
    expect(compact.length).toBeLessThan(20_000); // a 1000x1000 sheet is serialised sparsely
    wb.free();

    for (const json of [pretty, compact]) {
      const back = SpreadsheetWorkbook.fromJson(json);
      expect(back.sheetNames()).toEqual(["Main", "Data", "Summary"]);
      expect(back.sheetSize(1)).toEqual({ rows: 50, cols: 10 });
      expect(back.getDisplayValue(0, "C1")).toBe("40");
      expect(back.getFormula(0, "C1")).toBe("C1=SUM(A1:B1)");
      expect(back.getDisplayValue(0, "D1")).toBe("hello world");
      expect(back.getDisplayValue(1, "A1")).toBe("Err");
      expect(back.getInput(2, "A1")).toBe("2"); // SLEEP(const) is stored as its value, like the desktop
      // history is not part of the file
      expect(back.canUndo).toBe(false);
      // dependency graph survived: editing A1 recalculates B1 and the range in C1
      back.editCell(0, "A1", "1");
      expect(back.getDisplayValue(0, "B1")).toBe("3");
      expect(back.getDisplayValue(0, "C1")).toBe("4");
      back.free();
    }
  });

  it("loads the old desktop .290 layout (full boolean grid)", () => {
    const wb = SpreadsheetWorkbook.create(3, 3);
    wb.editCell(0, "A1", "2");
    wb.editCell(0, "C3", "=SUM(A1:B2)");
    const doc = JSON.parse(wb.toJson()) as Array<{ sheet: { buul: unknown } }>;
    doc[0].sheet.buul = [
      [true, true, false],
      [true, true, false],
      [false, false, false],
    ];
    wb.free();
    const back = SpreadsheetWorkbook.fromJson(JSON.stringify(doc));
    expect(back.getDisplayValue(0, "C3")).toBe("2");
    back.editCell(0, "B2", "5");
    expect(back.getDisplayValue(0, "C3")).toBe("7");
    back.free();
  });

  it("undo / redo is workbook-wide and replayed by the engine", () => {
    const wb = SpreadsheetWorkbook.create(10, 10);
    wb.addSheet("Two", 5, 5);
    wb.editCell(0, "A1", "1");
    wb.editCell(0, "B1", "=A1+1");
    wb.editCell(1, "A1", "x");
    wb.clearRange(0, { row1: 0, col1: 0, row2: 0, col2: 1 });
    expect(wb.getInput(0, "B1")).toBe("");

    let out = wb.undo();
    expect(out).toMatchObject({ sheet: 0, label: "clear 2 cell(s)" });
    expect(wb.getDisplayValue(0, "B1")).toBe("2");
    out = wb.undo();
    expect(out).toMatchObject({ sheet: 1, references: ["A1"] });
    expect(wb.getInput(1, "A1")).toBe("");
    wb.undo();
    wb.undo();
    expect(wb.canUndo).toBe(false);
    expect(wb.undo()).toBeNull();

    expect(wb.redo()?.label).toBe("edit A1");
    expect(wb.redo()?.label).toBe("edit B1");
    expect(wb.getDisplayValue(0, "B1")).toBe("2");
    wb.editCell(0, "C1", "3"); // new edit drops the redo branch
    expect(wb.canRedo).toBe(false);

    // cut/paste + undo restores rewritten formulas
    wb.moveCells(0, { row1: 0, col1: 0, row2: 0, col2: 0 }, 4, 4);
    expect(wb.getFormula(0, "B1")).toBe("B1=E5+1");
    wb.undo();
    expect(wb.getFormula(0, "B1")).toBe("B1=A1+1");
    expect(wb.getInput(0, "E5")).toBe("");

    wb.removeSheet(1);
    expect(wb.canUndo).toBe(false); // deleting a sheet clears history (desktop behaviour)
    wb.free();
  });

  it("find / replace / sort through the bridge", () => {
    const wb = SpreadsheetWorkbook.create(10, 10);
    wb.pasteTextBlock(0, 0, 0, [
      ["3", "c"],
      ["1", "a"],
      ["2", "b"],
    ]);
    wb.editCell(0, "D1", "=A1*10");
    // "find" looks at display and formula text, case-insensitively: D1's formula A1*10 matches too
    expect(wb.find(0, "a").map((c) => c.reference)).toEqual(["D1", "B2"]);
    expect(wb.find(0, "a", true).map((c) => c.reference)).toEqual(["B2"]);
    expect(wb.findReplace(0, "c", "z")).toEqual({ matched: 1, replaced: 1 });
    expect(wb.getDisplayValue(0, "B1")).toBe("z");
    expect(wb.sortRange(0, { row1: 0, col1: 0, row2: 2, col2: 1 }, "A", true).ok).toBe(true);
    expect([1, 2, 3].map((r) => wb.getDisplayValue(0, `B${r}`))).toEqual(["a", "b", "z"]);
    expect(wb.getDisplayValue(0, "D1")).toBe("10"); // dependent of A1 recalculated
    expect(wb.sortRange(0, { row1: 0, col1: 0, row2: 2, col2: 3 }, "A", true).status).toBe("range has constraints");
    expect(wb.sortCommand(0, "SORT(A1:B3;A;desc)")?.ok).toBe(true);
    expect(wb.getDisplayValue(0, "A1")).toBe("3");
    expect(wb.sortCommand(0, "hello")).toBeNull();
    wb.free();
  });

  it("formatting belongs to the workbook: patches, undo, cut/copy, save/load", () => {
    const wb = SpreadsheetWorkbook.create(10, 10);
    wb.editCell(0, "A1", "5");
    wb.editCell(0, "B1", "=A1*2");
    expect(wb.getFormat(0, "A1")).toEqual({});
    wb.setFormat(0, { row1: 0, col1: 0, row2: 0, col2: 1 }, { bold: true, fill: "#ffff00", align: "right" });
    expect(wb.getFormat(0, "B1")).toEqual({ bold: true, fill: "#ffff00", align: "right" });
    wb.setFormat(0, { row1: 0, col1: 0, row2: 0, col2: 0 }, { fill: null, fontSize: 18 });
    expect(wb.getFormat(0, "A1")).toEqual({ bold: true, fontSize: 18, align: "right" });
    expect(wb.getDisplayValue(0, "B1")).toBe("10"); // values untouched
    expect(wb.range(0, 0, 0, 0, 1).map((c) => c.format?.bold)).toEqual([true, true]);

    expect(wb.undo()?.label).toBe("format 1 cell(s)");
    expect(wb.getFormat(0, "A1")).toEqual({ bold: true, fill: "#ffff00", align: "right" });
    wb.redo();
    expect(wb.getFormat(0, "A1").fontSize).toBe(18);

    wb.moveCells(0, { row1: 0, col1: 0, row2: 0, col2: 0 }, 3, 3); // cut A1 -> D4
    expect(wb.getFormat(0, "D4").fontSize).toBe(18);
    expect(wb.getFormat(0, "A1")).toEqual({});
    wb.copyValues(0, { row1: 3, col1: 3, row2: 3, col2: 3 }, 5, 5); // copy D4 -> F6
    expect(wb.getFormat(0, "F6").bold).toBe(true);

    const back = SpreadsheetWorkbook.fromJson(wb.toJsonPretty());
    expect(back.getFormat(0, "D4")).toEqual(wb.getFormat(0, "D4"));
    expect(back.getFormat(0, "B1").fill).toBe("#ffff00");
    wb.free();
    back.free();
  });

  it("charts are stored per sheet and read live values from Rust", () => {
    const wb = SpreadsheetWorkbook.create(10, 10);
    wb.pasteTextBlock(0, 0, 0, [
      ["x", "y"],
      ["1", "10"],
      ["2", "=B2*2"],
    ]);
    const draft = {
      kind: "line" as const,
      title: "T",
      xLabel: "",
      yLabel: "",
      range: { row1: 0, col1: 0, row2: 2, col2: 1 },
      seriesInRows: false,
      headerLabels: true,
      firstSeriesIsX: true,
    };
    const data = wb.chartData(0, draft);
    expect(data.categories).toEqual(["1", "2"]);
    expect(data.series).toEqual([{ name: "y", values: [10, 20] }]);
    const id = wb.addChart(0, draft);
    expect(id).toBe(1);
    expect(wb.charts(0)[0]).toMatchObject({ id: 1, kind: "line", title: "T" });
    wb.editCell(0, "B2", "50");
    expect(wb.chartData(0, wb.charts(0)[0]).series[0].values).toEqual([50, 100]);
    wb.updateChart(0, { ...wb.charts(0)[0], kind: "scatter" });
    const back = SpreadsheetWorkbook.fromJson(wb.toJson());
    expect(back.charts(0)[0].kind).toBe("scatter");
    expect(back.charts(1 - 1)).toHaveLength(1);
    back.removeChart(0, 1);
    expect(back.charts(0)).toEqual([]);
    expect(() => back.removeChart(0, 1)).toThrow(/no chart/);
    wb.free();
    back.free();
  });

  it("cellRef helper matches the Rust column naming", () => {
    expect(cellRef(0, 0)).toBe("A1");
    expect(cellRef(2, 25)).toBe("Z3");
    expect(cellRef(0, 26)).toBe("AA1");
    expect(cellRef(9, 27)).toBe("AB10");
  });
});
