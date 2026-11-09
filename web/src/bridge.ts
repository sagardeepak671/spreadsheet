/**
 * Typed, ergonomic wrapper over the wasm-bindgen bindings in `./wasm-pkg`.
 *
 * Layering:
 *
 *   browser UI  →  bridge.ts (this file)  →  wasm-pkg/spreadsheet_wasm.js
 *               →  crates/wasm (Rust glue) →  crates/core (the engine)
 *
 * No spreadsheet logic lives here: this file only (1) loads the wasm module,
 * (2) turns the bridge's JSON strings into typed objects and (3) gives the
 * raw `WasmWorkbook` a small, idiomatic TypeScript face.
 */

import init, {
  initSync,
  version as wasmVersion,
  WasmWorkbook,
  type EditResult as WasmEditResult,
  type InitInput,
  type SyncInitInput,
} from "./wasm-pkg/spreadsheet_wasm.js";

/** Mirrors `spreadsheet_core::workbook::EditResult`. */
export interface EditResult {
  /** `true` when the engine reported "ok". */
  ok: boolean;
  /** Engine status text: "ok", "Invalid cmd", "circular error", "err". */
  status: string;
  /** Seconds a SLEEP(..) formula asked for (the engine never sleeps itself). */
  sleepSeconds: number;
}

/** Mirrors `spreadsheet_core::workbook::CellSnapshot` (camelCase via serde). */
export interface CellSnapshot {
  row: number;
  col: number;
  reference: string;
  display: string;
  value: number;
  isError: boolean;
  isString: boolean;
  hasFormula: boolean;
  formula: string;
  /** Editor text: `A1+5`, `"hello"`, `5`. */
  input: string;
  /** Visual formatting, when any was set. */
  format?: CellFormat;
}

export type Align = "left" | "center" | "right";

/** Mirrors `spreadsheet_core::CellFormat` (only non-default keys are present). */
export interface CellFormat {
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  fontSize?: number;
  color?: string;
  fill?: string;
  align?: Align;
}

/** Mirrors `spreadsheet_core::workbook::FormatPatch`: missing = leave, null = clear. */
export interface FormatPatch {
  bold?: boolean;
  italic?: boolean;
  underline?: boolean;
  fontSize?: number | null;
  color?: string | null;
  fill?: string | null;
  align?: Align | null;
  clearAll?: boolean;
}

export type ChartKind = "line" | "bar" | "scatter";

/** Mirrors `spreadsheet_core::charts::ChartSpec`. */
export interface ChartSpec {
  id: number;
  kind: ChartKind;
  title: string;
  xLabel: string;
  yLabel: string;
  range: Rect;
  seriesInRows: boolean;
  headerLabels: boolean;
  firstSeriesIsX: boolean;
}

export interface ChartSeries {
  name: string;
  values: Array<number | null>;
}

/** Mirrors `spreadsheet_core::charts::ChartData`: live values read from the cells. */
export interface ChartData {
  kind: ChartKind;
  title: string;
  xLabel: string;
  yLabel: string;
  categories: string[];
  x: Array<number | null> | null;
  series: ChartSeries[];
  warnings: string[];
}

/** Mirrors `spreadsheet_core::workbook::SheetSnapshot`. */
export interface SheetSnapshot {
  name: string;
  rows: number;
  cols: number;
  /** Non-empty cells only, row-major. */
  cells: CellSnapshot[];
}

/** Mirrors `spreadsheet_core::workbook::HistoryOutcome`. */
export interface HistoryOutcome {
  label: string;
  sheet: number;
  references: string[];
  result: EditResult;
}

/** Mirrors `spreadsheet_core::workbook::ReplaceOutcome`. */
export interface ReplaceOutcome {
  matched: number;
  replaced: number;
}

/** Inclusive 0-based rectangle, in the order the bridge takes it. */
export interface Rect {
  row1: number;
  col1: number;
  row2: number;
  col2: number;
}

let loaded = false;

/**
 * Loads the wasm module (async; fetches the `.wasm` next to the JS glue when
 * no input is given). Safe to call more than once.
 */
export async function loadSpreadsheetWasm(input?: InitInput): Promise<void> {
  if (loaded) return;
  await (input === undefined ? init() : init({ module_or_path: input }));
  loaded = true;
}

/**
 * Synchronous loader for environments where the bytes are already at hand
 * (Node tests: `fs.readFileSync(...)`).
 */
export function loadSpreadsheetWasmSync(module: SyncInitInput): void {
  if (loaded) return;
  initSync({ module });
  loaded = true;
}

export function bridgeVersion(): string {
  return wasmVersion();
}

function toEditResult(r: WasmEditResult): EditResult {
  // Copy the getters out of the wasm object so the wasm-side allocation can
  // be freed immediately instead of waiting for the FinalizationRegistry.
  const result = { ok: r.ok, status: r.status, sleepSeconds: r.sleepSeconds };
  r.free();
  return result;
}

/**
 * A workbook living in wasm memory. Every method forwards to Rust.
 *
 * Engine-level problems (bad formula, circular reference) come back as an
 * `EditResult` with `ok === false`; API misuse (unknown sheet, bad cell
 * reference, invalid JSON) throws an `Error`.
 */
export class SpreadsheetWorkbook {
  private constructor(private readonly raw: WasmWorkbook) {}

  /** One sheet called "Sheet 1" of `rows x cols`. */
  static create(rows: number, cols: number): SpreadsheetWorkbook {
    return new SpreadsheetWorkbook(new WasmWorkbook(rows, cols));
  }

  /** No sheets; call `addSheet`. */
  static empty(): SpreadsheetWorkbook {
    return new SpreadsheetWorkbook(WasmWorkbook.empty());
  }

  /** Loads the JSON produced by `toJson()` (same format as desktop `.290` files). */
  static fromJson(json: string): SpreadsheetWorkbook {
    return new SpreadsheetWorkbook(WasmWorkbook.fromJson(json));
  }

  /** Releases the wasm memory. The instance must not be used afterwards. */
  free(): void {
    this.raw.free();
  }

  // ---------------------------------------------------------------- sheets

  get sheetCount(): number {
    return this.raw.sheetCount();
  }

  sheetNames(): string[] {
    return this.raw.sheetNames();
  }

  sheetName(sheet: number): string {
    return this.raw.sheetName(sheet);
  }

  sheetSize(sheet: number): { rows: number; cols: number } {
    return { rows: this.raw.sheetRows(sheet), cols: this.raw.sheetCols(sheet) };
  }

  /** Appends an empty sheet and returns its index. */
  addSheet(name: string, rows: number, cols: number): number {
    return this.raw.addSheet(name, rows, cols);
  }

  /** Appends a sheet parsed from CSV text and returns its index. */
  addSheetFromCsv(name: string, csvText: string): number {
    return this.raw.addSheetFromCsv(name, csvText);
  }

  removeSheet(sheet: number): void {
    this.raw.removeSheet(sheet);
  }

  renameSheet(sheet: number, name: string): void {
    this.raw.renameSheet(sheet, name);
  }

  // --------------------------------------------------------------- editing

  /**
   * Sets `reference` ("A1") to `input`, the right-hand side of a formula:
   * `5`, `B1+5`, `SUM(A1:B2)`, `"text"`, `SLEEP(2)`.
   */
  setCell(sheet: number, reference: string, input: string): EditResult {
    return toEditResult(this.raw.setCell(sheet, reference, input));
  }

  /** Stores `text` as a string cell (quotes added for you). */
  setCellString(sheet: number, reference: string, text: string): EditResult {
    return toEditResult(this.raw.setCellString(sheet, reference, text));
  }

  /**
   * Spreadsheet-style edit (what the grid editor uses): "" clears the cell,
   * a leading "=" means strict formula, anything else is tried as a
   * formula / number and otherwise stored as text. Policy lives in Rust.
   */
  editCell(sheet: number, reference: string, text: string): EditResult {
    return toEditResult(this.raw.editCell(sheet, reference, text));
  }

  /** Resets a cell to "never written" and recalculates its dependents. */
  clearCell(sheet: number, reference: string): EditResult {
    return toEditResult(this.raw.clearCell(sheet, reference));
  }

  /** Runs a raw command of the command language (`A1=B1+5`, `scroll_toC3`, ...). */
  executeCommand(sheet: number, command: string): EditResult {
    return toEditResult(this.raw.executeCommand(sheet, command));
  }

  /** Clears every cell of the rectangle (one undo step). */
  clearRange(sheet: number, r: Rect): EditResult {
    return toEditResult(this.raw.clearRange(sheet, r.row1, r.col1, r.row2, r.col2));
  }

  /** Pastes a block of editor texts (system clipboard) at `row, col` (one undo step). */
  pasteTextBlock(sheet: number, row: number, col: number, block: string[][]): EditResult {
    return toEditResult(this.raw.pasteTextBlock(sheet, row, col, JSON.stringify(block)));
  }

  /** Desktop cut+paste: moves cells; formulas referencing them follow. */
  moveCells(sheet: number, from: Rect, toRow: number, toCol: number): EditResult {
    return toEditResult(this.raw.moveCells(sheet, from.row1, from.col1, from.row2, from.col2, toRow, toCol));
  }

  /** Desktop copy+paste: pastes current values (formulas are not copied). */
  copyValues(sheet: number, from: Rect, toRow: number, toCol: number): EditResult {
    return toEditResult(this.raw.copyValues(sheet, from.row1, from.col1, from.row2, from.col2, toRow, toCol));
  }

  /** Cells whose text contains `needle` (row-major). */
  find(sheet: number, needle: string, exact = false): CellSnapshot[] {
    return JSON.parse(this.raw.findJson(sheet, needle, exact)) as CellSnapshot[];
  }

  /** Desktop Find & Replace (exact match on constant cells, one undo step). */
  findReplace(sheet: number, find: string, replace: string): ReplaceOutcome {
    return JSON.parse(this.raw.findReplace(sheet, find, replace)) as ReplaceOutcome;
  }

  /** Desktop Sort: `key` = column name (sort rows) or 1-based row number (sort columns). */
  sortRange(sheet: number, r: Rect, key: string, ascending: boolean): EditResult {
    return toEditResult(this.raw.sortRange(sheet, r.row1, r.col1, r.row2, r.col2, key, ascending));
  }

  /** Desktop `SORT(A1:B5;A;asc)` command; null when `text` is not one. */
  sortCommand(sheet: number, text: string): EditResult | null {
    const r = this.raw.sortCommand(sheet, text);
    return r === undefined ? null : toEditResult(r);
  }

  // ------------------------------------------------------------ formatting

  getFormat(sheet: number, reference: string): CellFormat {
    return JSON.parse(this.raw.getFormat(sheet, reference)) as CellFormat;
  }

  /** Applies a formatting patch to a rectangle (one undo step, values untouched). */
  setFormat(sheet: number, r: Rect, patch: FormatPatch): EditResult {
    return toEditResult(this.raw.setFormat(sheet, r.row1, r.col1, r.row2, r.col2, JSON.stringify(patch)));
  }

  // ---------------------------------------------------------------- charts

  charts(sheet: number): ChartSpec[] {
    return JSON.parse(this.raw.chartsJson(sheet)) as ChartSpec[];
  }

  /** Stores a chart definition on the sheet; returns the assigned id. */
  addChart(sheet: number, spec: Omit<ChartSpec, "id">): number {
    return this.raw.addChart(sheet, JSON.stringify({ ...spec, id: 0 }));
  }

  updateChart(sheet: number, spec: ChartSpec): void {
    this.raw.updateChart(sheet, JSON.stringify(spec));
  }

  removeChart(sheet: number, id: number): void {
    this.raw.removeChart(sheet, id);
  }

  /** Live chart data for a (stored or draft) definition, read from the cells. */
  chartData(sheet: number, spec: Omit<ChartSpec, "id"> & { id?: number }): ChartData {
    return JSON.parse(this.raw.chartDataJson(sheet, JSON.stringify({ id: 0, ...spec }))) as ChartData;
  }

  // --------------------------------------------------------------- history

  get canUndo(): boolean {
    return this.raw.canUndo();
  }

  get canRedo(): boolean {
    return this.raw.canRedo();
  }

  clearHistory(): void {
    this.raw.clearHistory();
  }

  /** Reverts the last operation (any sheet); null when nothing to undo. */
  undo(): HistoryOutcome | null {
    const json = this.raw.undo();
    return json === undefined ? null : (JSON.parse(json) as HistoryOutcome);
  }

  redo(): HistoryOutcome | null {
    const json = this.raw.redo();
    return json === undefined ? null : (JSON.parse(json) as HistoryOutcome);
  }

  // --------------------------------------------------------------- reading

  /** Grid text: "Err", the string, or the number ("0" for untouched cells). */
  getDisplayValue(sheet: number, reference: string): string {
    return this.raw.getDisplayValue(sheet, reference);
  }

  /** Numeric value (0 for untouched / string / error cells). */
  getValue(sheet: number, reference: string): number {
    return this.raw.getValue(sheet, reference);
  }

  isError(sheet: number, reference: string): boolean {
    return this.raw.isError(sheet, reference);
  }

  /** Full command that recreates the cell, e.g. "B2=A1+5". */
  getFormula(sheet: number, reference: string): string {
    return this.raw.getFormula(sheet, reference);
  }

  /** Editor text for a cell: `A1+5`, `"hello"`, `5` ("" when untouched). */
  getInput(sheet: number, reference: string): string {
    return this.raw.getInput(sheet, reference);
  }

  /**
   * Non-empty cells inside the inclusive 0-based rectangle, clipped to the
   * sheet. The virtualized grid calls this for the visible window only.
   */
  range(sheet: number, row1: number, col1: number, row2: number, col2: number): CellSnapshot[] {
    return JSON.parse(this.raw.rangeJson(sheet, row1, col1, row2, col2)) as CellSnapshot[];
  }

  /** Dimensions plus every non-empty cell of a sheet. */
  snapshot(sheet: number): SheetSnapshot {
    return JSON.parse(this.raw.snapshotJson(sheet)) as SheetSnapshot;
  }

  // ----------------------------------------------------------- persistence

  /** All sheets incl. formulas and the dependency graph. */
  toJson(): string {
    return this.raw.toJson();
  }

  /** Same, pretty-printed like the desktop `.290` files. */
  toJsonPretty(): string {
    return this.raw.toJsonPretty();
  }

  /** Current values of one sheet as CSV text. */
  toCsv(sheet: number): string {
    return this.raw.toCsv(sheet);
  }
}

/** `(0, 0)` → "A1". Pure helper for UIs; the engine has its own copy in Rust. */
export function cellRef(row: number, col: number): string {
  let name = "";
  let c = col;
  while (c >= 0) {
    name = String.fromCharCode(65 + (c % 26)) + name;
    c = Math.floor(c / 26) - 1;
  }
  return `${name}${row + 1}`;
}
