/**
 * Clipboard semantics against the real wasm workbook (Node).
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";

import { loadSpreadsheetWasmSync, SpreadsheetWorkbook } from "./bridge";
import { ClipboardController, type SystemClipboard } from "./clipboard";
import { SheetAdapter } from "./grid/sheet-adapter";

beforeAll(() => {
  const wasmPath = fileURLToPath(new URL("./wasm-pkg/spreadsheet_wasm_bg.wasm", import.meta.url));
  loadSpreadsheetWasmSync(readFileSync(wasmPath));
});

function fakeSystem(initial = ""): SystemClipboard & { text: string; fail: boolean } {
  const sys = {
    text: initial,
    fail: false,
    writeText: async (t: string) => {
      if (sys.fail) throw new Error("denied");
      sys.text = t;
    },
    readText: async () => {
      if (sys.fail) throw new Error("denied");
      return sys.text;
    },
  };
  return sys;
}

describe("ClipboardController", () => {
  it("cut + paste moves cells and rewires references (desktop behaviour)", async () => {
    const wb = SpreadsheetWorkbook.create(10, 10);
    const sheet = new SheetAdapter(wb, 0);
    wb.editCell(0, "A1", "5");
    wb.editCell(0, "B1", "=A1*2");
    const sys = fakeSystem();
    const clip = new ClipboardController(sys);
    expect(await clip.copy(sheet, { r1: 0, c1: 0, r2: 0, c2: 0 }, "cut")).toBe(1);
    expect(sys.text).toBe("5"); // display text mirrored to the system clipboard
    const out = await clip.paste(sheet, { row: 3, col: 3 });
    expect(out?.kind).toBe("move");
    expect(out?.rect).toEqual({ r1: 3, c1: 3, r2: 3, c2: 3 });
    expect(wb.getInput(0, "D4")).toBe("5");
    expect(wb.getFormula(0, "B1")).toBe("B1=D4*2");
    expect(clip.current).toBeNull(); // a cut pastes once
    expect(await clip.paste(sheet, { row: 5, col: 5 })).toMatchObject({ kind: "text" }); // system text "5" remains
    wb.free();
  });

  it("copy + paste pastes values, not formulas", async () => {
    const wb = SpreadsheetWorkbook.create(10, 10);
    const sheet = new SheetAdapter(wb, 0);
    wb.editCell(0, "A1", "5");
    wb.editCell(0, "A2", "=A1*2");
    const clip = new ClipboardController(fakeSystem());
    await clip.copy(sheet, { r1: 0, c1: 0, r2: 1, c2: 0 }, "copy");
    const out = await clip.paste(sheet, { row: 0, col: 2 });
    expect(out?.kind).toBe("values");
    expect(wb.getInput(0, "C1")).toBe("5");
    expect(wb.getInput(0, "C2")).toBe("10");
    expect(clip.current?.mode).toBe("copy"); // copy can be pasted again
    await clip.paste(sheet, { row: 0, col: 4 });
    expect(wb.getInput(0, "E2")).toBe("10");
    wb.free();
  });

  it("pastes foreign clipboard text cell by cell with editing rules", async () => {
    const wb = SpreadsheetWorkbook.create(10, 10);
    const sheet = new SheetAdapter(wb, 0);
    const sys = fakeSystem("1\t2\n=A1+B1\thello\n");
    const clip = new ClipboardController(sys);
    const out = await clip.paste(sheet, { row: 0, col: 0 });
    expect(out?.kind).toBe("text");
    expect(out?.rect).toEqual({ r1: 0, c1: 0, r2: 1, c2: 1 });
    expect(wb.getValue(0, "A2")).toBe(3);
    expect(wb.getDisplayValue(0, "B2")).toBe("hello");
    // the engine made it one undo step
    expect(wb.undo()?.label).toBe("paste 2 row(s)");
    expect(wb.getInput(0, "A2")).toBe("");
    wb.free();
  });

  it("falls back to the in-app clipboard when the system clipboard is unavailable", async () => {
    const wb = SpreadsheetWorkbook.create(5, 5);
    const sheet = new SheetAdapter(wb, 0);
    wb.editCell(0, "A1", "7");
    const sys = fakeSystem();
    sys.fail = true;
    const clip = new ClipboardController(sys);
    await clip.copy(sheet, { r1: 0, c1: 0, r2: 0, c2: 0 }, "copy");
    const out = await clip.paste(sheet, { row: 1, col: 1 });
    expect(out?.kind).toBe("values");
    expect(wb.getValue(0, "B2")).toBe(7);
    expect(await new ClipboardController(fakeSystem()).paste(sheet, { row: 0, col: 0 })).toBeNull();
    wb.free();
  });

  it("pastes across sheets as text", async () => {
    const wb = SpreadsheetWorkbook.create(5, 5);
    wb.addSheet("Two", 5, 5);
    wb.editCell(0, "A1", "9");
    const clip = new ClipboardController(fakeSystem());
    await clip.copy(new SheetAdapter(wb, 0), { r1: 0, c1: 0, r2: 0, c2: 0 }, "copy");
    const out = await clip.paste(new SheetAdapter(wb, 1), { row: 0, col: 0 });
    expect(out?.kind).toBe("text");
    expect(wb.getValue(1, "A1")).toBe(9);
    wb.free();
  });
});
