import { describe, expect, it } from "vitest";

import {
  cellAtPoint,
  cellRef,
  clampPos,
  columnIndex,
  columnName,
  DEFAULT_GEOMETRY as geo,
  isMacPlatform,
  keyToEditorAction,
  keyToGridAction,
  parseClipboardText,
  parseRef,
  scrollToReveal,
  selectionRect,
  toClipboardText,
  visibleRange,
  type KeyInfo,
} from "./model";

const key = (k: string, mods: Partial<KeyInfo> = {}): KeyInfo => ({
  key: k,
  shiftKey: false,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
});

describe("cell naming", () => {
  it("matches the Rust column naming in both directions", () => {
    for (const [i, name] of [
      [0, "A"],
      [25, "Z"],
      [26, "AA"],
      [27, "AB"],
      [701, "ZZ"],
      [702, "AAA"],
      [18277, "ZZZ"],
    ] as const) {
      expect(columnName(i)).toBe(name);
      expect(columnIndex(name)).toBe(i);
    }
    expect(cellRef({ row: 2, col: 1 })).toBe("B3");
  });

  it("parses references leniently", () => {
    expect(parseRef("B3")).toEqual({ row: 2, col: 1 });
    expect(parseRef(" aa10 ")).toEqual({ row: 9, col: 26 });
    expect(parseRef("A0")).toBeNull();
    expect(parseRef("3B")).toBeNull();
    expect(parseRef("")).toBeNull();
  });
});

describe("selection", () => {
  it("normalises rectangles regardless of drag direction", () => {
    expect(selectionRect({ anchor: { row: 5, col: 4 }, active: { row: 1, col: 2 } })).toEqual({ r1: 1, c1: 2, r2: 5, c2: 4 });
  });

  it("clamps to the sheet", () => {
    expect(clampPos({ row: -3, col: 2000 }, 1000, 1000)).toEqual({ row: 0, col: 999 });
  });
});

describe("virtualization window", () => {
  const vp = { scrollTop: 0, scrollLeft: 0, width: 1000, height: 520 };

  it("renders only the visible cells plus the buffer", () => {
    const rect = visibleRange(vp, geo, 1000, 1000, 3);
    // 520px / 26px = 20 rows, 1000px / 100px = 10 cols, + 3 buffer below/right
    expect(rect).toEqual({ r1: 0, c1: 0, r2: 22, c2: 12 });
  });

  it("moves with the scroll offset and is symmetric", () => {
    const rect = visibleRange({ ...vp, scrollTop: 26 * 500 + 13, scrollLeft: 100 * 300 + 50 }, geo, 1000, 1000, 2);
    expect(rect.r1).toBe(498);
    expect(rect.r2).toBe(522); // rows 500..520 are visible (partial last row), +2
    expect(rect.c1).toBe(298);
    expect(rect.c2).toBe(312);
  });

  it("never exceeds the sheet", () => {
    const rect = visibleRange({ ...vp, scrollTop: 26 * 990 }, geo, 1000, 5, 3);
    expect(rect.r2).toBe(999);
    expect(rect.c2).toBe(4);
    expect(visibleRange(vp, geo, 0, 0, 3).r2).toBe(-1);
  });

  it("keeps the DOM size bounded for a huge sheet", () => {
    const rect = visibleRange({ ...vp, width: 1920, height: 1080 }, geo, 1000, 18278, 3);
    const cells = (rect.r2 - rect.r1 + 1) * (rect.c2 - rect.c1 + 1);
    expect(cells).toBeLessThan(1500);
  });

  it("computes the minimal scroll to reveal a cell", () => {
    expect(scrollToReveal({ row: 5, col: 3 }, vp, geo)).toBeNull();
    expect(scrollToReveal({ row: 40, col: 0 }, vp, geo)).toEqual({ scrollTop: 41 * 26 - 520, scrollLeft: 0 });
    expect(scrollToReveal({ row: 0, col: 15 }, vp, geo)).toEqual({ scrollTop: 0, scrollLeft: 16 * 100 - 1000 });
    expect(scrollToReveal({ row: 2, col: 2 }, { ...vp, scrollTop: 200, scrollLeft: 300 }, geo)).toEqual({
      scrollTop: 52,
      scrollLeft: 200,
    });
  });

  it("maps pointer positions to cells", () => {
    expect(cellAtPoint(150, 30, geo, 100, 100)).toEqual({ row: 1, col: 1 });
    expect(cellAtPoint(-5, 99999, geo, 100, 100)).toEqual({ row: 99, col: 0 });
  });
});

describe("keyboard mapping", () => {
  it("navigation keys", () => {
    expect(keyToGridAction(key("ArrowDown"), false)).toEqual({ type: "move", dRow: 1, dCol: 0, extend: false });
    expect(keyToGridAction(key("ArrowLeft", { shiftKey: true }), false)).toEqual({ type: "move", dRow: 0, dCol: -1, extend: true });
    expect(keyToGridAction(key("Enter"), false)).toEqual({ type: "move", dRow: 1, dCol: 0, extend: false });
    expect(keyToGridAction(key("Enter", { shiftKey: true }), false)).toEqual({ type: "move", dRow: -1, dCol: 0, extend: false });
    expect(keyToGridAction(key("Tab"), false)).toEqual({ type: "move", dRow: 0, dCol: 1, extend: false });
    expect(keyToGridAction(key("Tab", { shiftKey: true }), false)).toEqual({ type: "move", dRow: 0, dCol: -1, extend: false });
    expect(keyToGridAction(key("Home"), false)).toEqual({ type: "jump", to: "rowStart", extend: false });
    expect(keyToGridAction(key("PageDown"), false)).toEqual({ type: "page", dir: 1, extend: false });
    expect(keyToGridAction(key("F2"), false)).toEqual({ type: "edit", replace: false });
    expect(keyToGridAction(key("Delete"), false)).toEqual({ type: "clear" });
    expect(keyToGridAction(key("x"), false)).toEqual({ type: "type", text: "x" });
    expect(keyToGridAction(key("="), false)).toEqual({ type: "type", text: "=" });
    expect(keyToGridAction(key("Shift"), false)).toBeNull();
  });

  it("uses Cmd on macOS and Ctrl elsewhere", () => {
    expect(keyToGridAction(key("z", { metaKey: true }), true)).toEqual({ type: "undo" });
    expect(keyToGridAction(key("z", { ctrlKey: true }), false)).toEqual({ type: "undo" });
    expect(keyToGridAction(key("z", { ctrlKey: true, shiftKey: true }), false)).toEqual({ type: "redo" });
    expect(keyToGridAction(key("y", { ctrlKey: true }), false)).toEqual({ type: "redo" });
    expect(keyToGridAction(key("c", { metaKey: true }), true)).toEqual({ type: "copy" });
    expect(keyToGridAction(key("v", { ctrlKey: true }), false)).toEqual({ type: "paste" });
    expect(keyToGridAction(key("x", { ctrlKey: true }), false)).toEqual({ type: "cut" });
    expect(keyToGridAction(key("a", { ctrlKey: true }), false)).toEqual({ type: "selectAll" });
    expect(keyToGridAction(key("g", { metaKey: true }), true)).toEqual({ type: "goto" });
    expect(keyToGridAction(key("Home", { ctrlKey: true }), false)).toEqual({ type: "jump", to: "sheetStart", extend: false });
    expect(keyToGridAction(key("ArrowDown", { metaKey: true, shiftKey: true }), true)).toEqual({ type: "jump", to: "down", extend: true });
    // Ctrl+key on mac is not a shortcut and should not start typing either
    expect(keyToGridAction(key("c", { ctrlKey: true }), true)).toBeNull();
  });

  it("editor keys depend on how editing started", () => {
    expect(keyToEditorAction(key("Enter"), false, true)).toEqual({ type: "commit", move: "down" });
    expect(keyToEditorAction(key("Enter", { shiftKey: true }), false, true)).toEqual({ type: "commit", move: "up" });
    expect(keyToEditorAction(key("Enter", { metaKey: true }), true, true)).toEqual({ type: "commit", move: "none" });
    expect(keyToEditorAction(key("Tab"), false, false)).toEqual({ type: "commit", move: "right" });
    expect(keyToEditorAction(key("Escape"), false, false)).toEqual({ type: "cancel" });
    // typing mode: arrows commit and move; F2 mode: arrows move the caret
    expect(keyToEditorAction(key("ArrowRight"), false, true)).toEqual({ type: "commit", move: "right" });
    expect(keyToEditorAction(key("ArrowRight"), false, false)).toBeNull();
    expect(keyToEditorAction(key("a"), false, true)).toBeNull();
  });

  it("detects macOS", () => {
    expect(isMacPlatform({ platform: "MacIntel" })).toBe(true);
    expect(isMacPlatform({ platform: "", userAgent: "Mozilla/5.0 (Macintosh; Intel Mac OS X)" })).toBe(true);
    expect(isMacPlatform({ platform: "Win32" })).toBe(false);
    expect(isMacPlatform({ platform: "Linux x86_64" })).toBe(false);
  });
});

describe("clipboard text", () => {
  it("round-trips tab separated blocks", () => {
    const block = [
      ["1", "A1+1", '"x"'],
      ["", "SUM(A1:B2)", ""],
    ];
    expect(parseClipboardText(toClipboardText(block))).toEqual(block);
  });

  it("accepts Windows line endings and a trailing newline", () => {
    expect(parseClipboardText("1\t2\r\n3\t4\r\n")).toEqual([
      ["1", "2"],
      ["3", "4"],
    ]);
    expect(parseClipboardText("solo")).toEqual([["solo"]]);
  });
});
