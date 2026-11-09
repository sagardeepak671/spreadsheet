/**
 * `SheetAdapter` is the single gateway between the grid UI and one sheet of
 * the Rust workbook. Every value the grid shows is read from Rust right after
 * the change, never from a JS-side cache, and every edit goes through the
 * core (`editCell`, `clearRange`, `moveCells`, ...). Undo/redo history lives
 * in the core as well (`Workbook::undo`), shared by all sheets.
 */

import type { CellSnapshot, EditResult, HistoryOutcome, Rect, ReplaceOutcome, SpreadsheetWorkbook } from "../bridge";
import { cellRef, type CellPos, type CellRect } from "./model";

export interface GridDataSource {
  readonly rows: number;
  readonly cols: number;
  /** Non-empty cells inside the rectangle (inclusive, clipped). */
  range(rect: CellRect): CellSnapshot[];
  /** Editor text of a cell ("" when untouched). */
  input(pos: CellPos): string;
  /** Display text of a cell ("" when untouched). */
  display(pos: CellPos): string;
  /** Spreadsheet-style edit of a single cell (undoable). */
  edit(pos: CellPos, text: string): EditResult;
  /** Clears every cell of the rectangle as one undoable transaction. */
  clearRect(rect: CellRect): EditResult;
}

export function toRect(rect: CellRect): Rect {
  return { row1: rect.r1, col1: rect.c1, row2: rect.r2, col2: rect.c2 };
}

export class SheetAdapter implements GridDataSource {
  readonly rows: number;
  readonly cols: number;

  constructor(
    readonly wb: SpreadsheetWorkbook,
    readonly sheet: number,
  ) {
    const size = wb.sheetSize(sheet);
    this.rows = size.rows;
    this.cols = size.cols;
  }

  get name(): string {
    return this.wb.sheetName(this.sheet);
  }

  range(rect: CellRect): CellSnapshot[] {
    if (rect.r1 > rect.r2 || rect.c1 > rect.c2) return [];
    return this.wb.range(this.sheet, rect.r1, rect.c1, rect.r2, rect.c2);
  }

  input(pos: CellPos): string {
    return this.wb.getInput(this.sheet, cellRef(pos));
  }

  display(pos: CellPos): string {
    const ref = cellRef(pos);
    // Untouched cells are implicitly 0 in the engine; the grid shows them blank.
    return this.wb.getInput(this.sheet, ref) === "" ? "" : this.wb.getDisplayValue(this.sheet, ref);
  }

  edit(pos: CellPos, text: string): EditResult {
    return this.wb.editCell(this.sheet, cellRef(pos), text);
  }

  clearRect(rect: CellRect): EditResult {
    return this.wb.clearRange(this.sheet, toRect(rect));
  }

  /** Display texts of a rectangle as a 2-D array (for the system clipboard). */
  displayBlock(rect: CellRect): string[][] {
    const byKey = new Map(this.range(rect).map((c) => [`${c.row},${c.col}`, c.display]));
    const block: string[][] = [];
    for (let r = rect.r1; r <= rect.r2; r++) {
      const line: string[] = [];
      for (let c = rect.c1; c <= rect.c2; c++) line.push(byKey.get(`${r},${c}`) ?? "");
      block.push(line);
    }
    return block;
  }

  pasteText(anchor: CellPos, block: string[][]): EditResult {
    return this.wb.pasteTextBlock(this.sheet, anchor.row, anchor.col, block);
  }

  /** Desktop cut+paste semantics (references follow the moved cells). */
  moveCells(from: CellRect, to: CellPos): EditResult {
    return this.wb.moveCells(this.sheet, toRect(from), to.row, to.col);
  }

  /** Desktop copy+paste semantics (values only). */
  copyValues(from: CellRect, to: CellPos): EditResult {
    return this.wb.copyValues(this.sheet, toRect(from), to.row, to.col);
  }

  find(needle: string, exact = false): CellSnapshot[] {
    return this.wb.find(this.sheet, needle, exact);
  }

  findReplace(find: string, replace: string): ReplaceOutcome {
    return this.wb.findReplace(this.sheet, find, replace);
  }

  sortRange(rect: CellRect, key: string, ascending: boolean): EditResult {
    return this.wb.sortRange(this.sheet, toRect(rect), key, ascending);
  }

  sortCommand(text: string): EditResult | null {
    return this.wb.sortCommand(this.sheet, text);
  }

  undo(): HistoryOutcome | null {
    return this.wb.undo();
  }

  redo(): HistoryOutcome | null {
    return this.wb.redo();
  }
}
