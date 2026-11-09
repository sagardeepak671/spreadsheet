/**
 * Pure, DOM-free grid logic: geometry, virtualization window, selection
 * rectangles, keyboard mapping and clipboard text. Everything here is unit
 * tested with vitest; `grid.ts` is the thin DOM layer on top.
 *
 * No spreadsheet semantics live here: values, formulas and recalculation
 * always come from the Rust workbook.
 */

export interface CellPos {
  row: number;
  col: number;
}

/** Inclusive, normalised rectangle (r1 <= r2, c1 <= c2). */
export interface CellRect {
  r1: number;
  c1: number;
  r2: number;
  c2: number;
}

/**
 * `active` is the cursor cell (where typing goes); `anchor` is where a range
 * selection started. A single-cell selection has anchor === active.
 */
export interface Selection {
  anchor: CellPos;
  active: CellPos;
}

export interface GridGeometry {
  rowHeight: number;
  colWidth: number;
  headerHeight: number;
  rowHeaderWidth: number;
}

export const DEFAULT_GEOMETRY: GridGeometry = {
  rowHeight: 26,
  colWidth: 100,
  headerHeight: 26,
  rowHeaderWidth: 52,
};

// ----------------------------------------------------------------- naming

/** 0-based column index -> "A", "Z", "AA" ... (same rule as the Rust core). */
export function columnName(col: number): string {
  let name = "";
  let c = col;
  while (c >= 0) {
    name = String.fromCharCode(65 + (c % 26)) + name;
    c = Math.floor(c / 26) - 1;
  }
  return name;
}

/** "AA" -> 26 (inverse of `columnName`). */
export function columnIndex(name: string): number {
  let col = -1;
  for (const ch of name) {
    col = (col + 1) * 26 + (ch.charCodeAt(0) - 65);
  }
  return col;
}

export function cellRef(pos: CellPos): string {
  return `${columnName(pos.col)}${pos.row + 1}`;
}

/** "b3" / " B3 " -> {row: 2, col: 1}; null when not a reference. */
export function parseRef(text: string): CellPos | null {
  const m = /^\s*([A-Za-z]+)\s*(\d+)\s*$/.exec(text);
  if (!m) return null;
  const row = Number(m[2]) - 1;
  if (row < 0) return null;
  return { row, col: columnIndex(m[1].toUpperCase()) };
}

// -------------------------------------------------------------- selection

export function samePos(a: CellPos, b: CellPos): boolean {
  return a.row === b.row && a.col === b.col;
}

export function clampPos(pos: CellPos, rows: number, cols: number): CellPos {
  return {
    row: Math.min(Math.max(pos.row, 0), rows - 1),
    col: Math.min(Math.max(pos.col, 0), cols - 1),
  };
}

export function selectionRect(sel: Selection): CellRect {
  return {
    r1: Math.min(sel.anchor.row, sel.active.row),
    c1: Math.min(sel.anchor.col, sel.active.col),
    r2: Math.max(sel.anchor.row, sel.active.row),
    c2: Math.max(sel.anchor.col, sel.active.col),
  };
}

export function isSingleCell(sel: Selection): boolean {
  return samePos(sel.anchor, sel.active);
}

export function rectContains(rect: CellRect, pos: CellPos): boolean {
  return pos.row >= rect.r1 && pos.row <= rect.r2 && pos.col >= rect.c1 && pos.col <= rect.c2;
}

export function rectCellCount(rect: CellRect): number {
  return (rect.r2 - rect.r1 + 1) * (rect.c2 - rect.c1 + 1);
}

// --------------------------------------------------------- virtualization

export interface Viewport {
  scrollTop: number;
  scrollLeft: number;
  width: number;
  height: number;
}

/**
 * Which cells to materialise in the DOM: the cells intersecting the viewport
 * plus `buffer` extra rows/cols on every side (so small scrolls don't pop).
 */
export function visibleRange(
  vp: Viewport,
  geo: GridGeometry,
  rows: number,
  cols: number,
  buffer: number,
): CellRect {
  if (rows <= 0 || cols <= 0) return { r1: 0, c1: 0, r2: -1, c2: -1 };
  const firstRow = Math.floor(vp.scrollTop / geo.rowHeight);
  const lastRow = Math.ceil((vp.scrollTop + vp.height) / geo.rowHeight) - 1;
  const firstCol = Math.floor(vp.scrollLeft / geo.colWidth);
  const lastCol = Math.ceil((vp.scrollLeft + vp.width) / geo.colWidth) - 1;
  return {
    r1: Math.max(0, firstRow - buffer),
    r2: Math.min(rows - 1, Math.max(firstRow, lastRow) + buffer),
    c1: Math.max(0, firstCol - buffer),
    c2: Math.min(cols - 1, Math.max(firstCol, lastCol) + buffer),
  };
}

/** Pixel rectangle of a cell inside the scrollable content. */
export function cellBox(pos: CellPos, geo: GridGeometry): { left: number; top: number; width: number; height: number } {
  return {
    left: pos.col * geo.colWidth,
    top: pos.row * geo.rowHeight,
    width: geo.colWidth,
    height: geo.rowHeight,
  };
}

/**
 * Minimal scroll offset change that brings `pos` fully into view, or null if
 * it is already visible.
 */
export function scrollToReveal(
  pos: CellPos,
  vp: Viewport,
  geo: GridGeometry,
): { scrollTop: number; scrollLeft: number } | null {
  const box = cellBox(pos, geo);
  let top = vp.scrollTop;
  let left = vp.scrollLeft;
  if (box.top < vp.scrollTop) top = box.top;
  else if (box.top + box.height > vp.scrollTop + vp.height) top = box.top + box.height - vp.height;
  if (box.left < vp.scrollLeft) left = box.left;
  else if (box.left + box.width > vp.scrollLeft + vp.width) left = box.left + box.width - vp.width;
  if (top === vp.scrollTop && left === vp.scrollLeft) return null;
  return { scrollTop: Math.max(0, top), scrollLeft: Math.max(0, left) };
}

/** Cell under a pixel position (content coordinates), clamped to the sheet. */
export function cellAtPoint(x: number, y: number, geo: GridGeometry, rows: number, cols: number): CellPos {
  return clampPos({ row: Math.floor(y / geo.rowHeight), col: Math.floor(x / geo.colWidth) }, rows, cols);
}

// ----------------------------------------------------------------- keys

export interface KeyInfo {
  key: string;
  shiftKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
}

export type GridAction =
  | { type: "move"; dRow: number; dCol: number; extend: boolean }
  | { type: "page"; dir: 1 | -1; extend: boolean }
  | { type: "jump"; to: "rowStart" | "rowEnd" | "sheetStart" | "sheetEnd" | "up" | "down" | "left" | "right"; extend: boolean }
  | { type: "edit"; replace: boolean }
  | { type: "clear" }
  | { type: "copy" }
  | { type: "cut" }
  | { type: "paste" }
  | { type: "undo" }
  | { type: "redo" }
  | { type: "selectAll" }
  | { type: "goto" }
  | { type: "type"; text: string };

export type EditorAction =
  | { type: "commit"; move: "down" | "up" | "right" | "left" | "none" }
  | { type: "cancel" };

/** Detects macOS so shortcuts use ⌘ instead of Ctrl. */
export function isMacPlatform(nav: { platform?: string; userAgent?: string }): boolean {
  const p = `${nav.platform ?? ""} ${nav.userAgent ?? ""}`;
  return /Mac|iPhone|iPad|iPod/.test(p);
}

/** The "primary" modifier: ⌘ on macOS, Ctrl elsewhere. */
export function primaryModifier(k: KeyInfo, isMac: boolean): boolean {
  return isMac ? k.metaKey : k.ctrlKey;
}

/**
 * Maps a keydown (grid focused, not editing) to an action. Returns null for
 * keys the grid does not handle so the browser default can run.
 */
export function keyToGridAction(k: KeyInfo, isMac: boolean): GridAction | null {
  const mod = primaryModifier(k, isMac);
  const ext = k.shiftKey;

  if (mod && !k.altKey) {
    switch (k.key.toLowerCase()) {
      case "c":
        return { type: "copy" };
      case "x":
        return { type: "cut" };
      case "v":
        return { type: "paste" };
      case "z":
        return ext ? { type: "redo" } : { type: "undo" };
      case "y":
        return { type: "redo" };
      case "a":
        return { type: "selectAll" };
      case "g":
        return { type: "goto" };
      case "home":
        return { type: "jump", to: "sheetStart", extend: ext };
      case "end":
        return { type: "jump", to: "sheetEnd", extend: ext };
      case "arrowup":
        return { type: "jump", to: "up", extend: ext };
      case "arrowdown":
        return { type: "jump", to: "down", extend: ext };
      case "arrowleft":
        return { type: "jump", to: "left", extend: ext };
      case "arrowright":
        return { type: "jump", to: "right", extend: ext };
      default:
        return null;
    }
  }

  switch (k.key) {
    case "ArrowUp":
      return { type: "move", dRow: -1, dCol: 0, extend: ext };
    case "ArrowDown":
      return { type: "move", dRow: 1, dCol: 0, extend: ext };
    case "ArrowLeft":
      return { type: "move", dRow: 0, dCol: -1, extend: ext };
    case "ArrowRight":
      return { type: "move", dRow: 0, dCol: 1, extend: ext };
    case "Enter":
      return { type: "move", dRow: ext ? -1 : 1, dCol: 0, extend: false };
    case "Tab":
      return { type: "move", dRow: 0, dCol: ext ? -1 : 1, extend: false };
    case "Home":
      return { type: "jump", to: "rowStart", extend: ext };
    case "End":
      return { type: "jump", to: "rowEnd", extend: ext };
    case "PageUp":
      return { type: "page", dir: -1, extend: ext };
    case "PageDown":
      return { type: "page", dir: 1, extend: ext };
    case "F2":
      return { type: "edit", replace: false };
    case "Delete":
    case "Backspace":
      return { type: "clear" };
    default:
      break;
  }

  // A printable character starts editing and replaces the content.
  if (k.key.length === 1 && !k.ctrlKey && !k.metaKey && !k.altKey) {
    return { type: "type", text: k.key };
  }
  return null;
}

/**
 * Maps a keydown inside the cell editor. `replaceMode` is true when editing
 * started by typing (arrows then commit + move, like Excel); in F2 /
 * double-click mode the arrows move the caret instead.
 */
export function keyToEditorAction(k: KeyInfo, isMac: boolean, replaceMode: boolean): EditorAction | null {
  const mod = primaryModifier(k, isMac);
  switch (k.key) {
    case "Enter":
      if (mod) return { type: "commit", move: "none" };
      return { type: "commit", move: k.shiftKey ? "up" : "down" };
    case "Tab":
      return { type: "commit", move: k.shiftKey ? "left" : "right" };
    case "Escape":
      return { type: "cancel" };
    case "ArrowUp":
      return replaceMode ? { type: "commit", move: "up" } : null;
    case "ArrowDown":
      return replaceMode ? { type: "commit", move: "down" } : null;
    case "ArrowLeft":
      return replaceMode ? { type: "commit", move: "left" } : null;
    case "ArrowRight":
      return replaceMode ? { type: "commit", move: "right" } : null;
    default:
      return null;
  }
}

export function moveDelta(move: "down" | "up" | "right" | "left" | "none"): { dRow: number; dCol: number } {
  switch (move) {
    case "down":
      return { dRow: 1, dCol: 0 };
    case "up":
      return { dRow: -1, dCol: 0 };
    case "right":
      return { dRow: 0, dCol: 1 };
    case "left":
      return { dRow: 0, dCol: -1 };
    default:
      return { dRow: 0, dCol: 0 };
  }
}

// ------------------------------------------------------------- clipboard

/** Tab/newline separated text -> 2-D array (Excel/Sheets paste format). */
export function parseClipboardText(text: string): string[][] {
  const normalised = text.replace(/\r\n?/g, "\n");
  const lines = normalised.endsWith("\n") ? normalised.slice(0, -1).split("\n") : normalised.split("\n");
  return lines.map((line) => line.split("\t"));
}

export function toClipboardText(block: string[][]): string {
  return block.map((row) => row.join("\t")).join("\n");
}

/** Shortcut labels for the help text, rendered with the platform modifier. */
export function shortcutHelp(isMac: boolean): Array<[string, string]> {
  const mod = isMac ? "⌘" : "Ctrl";
  return [
    ["Arrows", "move"],
    ["Shift+Arrows", "extend selection"],
    ["Enter / Shift+Enter", "commit, move down / up"],
    ["Tab / Shift+Tab", "commit, move right / left"],
    [`${mod}+Enter`, "commit, stay"],
    ["Esc", "cancel edit"],
    ["F2 / double-click", "edit in place"],
    ["Delete / Backspace", "clear selection"],
    ["Home / End", "first / last column"],
    [`${mod}+Home / ${mod}+End`, "A1 / last cell"],
    [`${mod}+Arrows`, "jump to sheet edge"],
    ["PageUp / PageDown", "move one screen"],
    [`${mod}+C / ${mod}+X / ${mod}+V`, "copy / cut / paste"],
    [`${mod}+Z / ${mod}+Shift+Z`, "undo / redo"],
    [`${mod}+A`, "select all"],
    [`${mod}+G`, "go to cell"],
    [`${mod}+B / ${mod}+I / ${mod}+U`, "bold / italic / underline"],
    ["Right-click", "context menu"],
  ];
}
