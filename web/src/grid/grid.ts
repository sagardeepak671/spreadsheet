/**
 * Virtualized spreadsheet grid (DOM layer).
 *
 * Only the cells intersecting the viewport (plus a small buffer) exist in the
 * DOM. A fixed pool of cell elements is re-positioned and re-filled on every
 * scroll frame, so a 1000 x 18278 sheet costs the same as a 30 x 15 one.
 *
 * Text shown in a cell is always fetched from the Rust workbook through the
 * `GridDataSource` (`range()` for the visible window) right after scrolling
 * or editing - there is no JS-side cache of values.
 */

import type { CellSnapshot, EditResult } from "../bridge";
import { formatToCss } from "../format";
import {
  cellAtPoint,
  cellBox,
  cellRef,
  clampPos,
  columnName,
  DEFAULT_GEOMETRY,
  isMacPlatform,
  isSingleCell,
  keyToEditorAction,
  keyToGridAction,
  moveDelta,
  rectCellCount,
  rectContains,
  samePos,
  scrollToReveal,
  selectionRect,
  visibleRange,
  type CellPos,
  type CellRect,
  type GridAction,
  type GridGeometry,
  type Selection,
  type Viewport,
} from "./model";
import type { GridDataSource } from "./sheet-adapter";

export interface GridOptions {
  geometry?: GridGeometry;
  /** Extra rows/cols rendered beyond the viewport on each side. */
  buffer?: number;
  isMac?: boolean;
  scrollBehavior?: ScrollBehavior;
}

/** Workbook-level commands the grid delegates to its host. */
export type GridCommand = "copy" | "cut" | "paste" | "undo" | "redo" | "goto";

export interface GridEvents {
  onSelectionChange?(sel: Selection): void;
  /** Copy/cut/paste/undo/redo/goto: implemented by the host (they span sheets / the clipboard). */
  onCommand?(command: GridCommand): void;
  /** Fired after any change to the workbook (edit, clear, paste, undo...). */
  onDataChange?(result: EditResult, what: string): void;
  /** Live editor text while editing in-cell; `null` when editing stops. */
  onEditingChange?(text: string | null): void;
  onStatus?(message: string, kind: "ok" | "error" | "info"): void;
  /** Right-click on the grid (the clicked cell is already selected). */
  onContextMenu?(clientX: number, clientY: number): void;
}

export interface RenderStats {
  rect: CellRect;
  renderedCells: number;
}

interface PooledCell {
  el: HTMLDivElement;
  row: number;
  col: number;
  text: string;
  cls: string;
  /** Inline CSS derived from the cell's format ("" = default). */
  css: string;
}

interface PooledHeader {
  el: HTMLDivElement;
  index: number;
  text: string;
  hl: boolean;
}

const EMPTY_RECT: CellRect = { r1: 0, c1: 0, r2: -1, c2: -1 };

export class SpreadsheetGrid {
  readonly geo: GridGeometry;
  private readonly buffer: number;
  private readonly isMac: boolean;
  private readonly scrollBehavior: ScrollBehavior;

  private readonly viewport: HTMLDivElement;
  private readonly content: HTMLDivElement;
  private readonly cellLayer: HTMLDivElement;
  private readonly colTrack: HTMLDivElement;
  private readonly rowTrack: HTMLDivElement;
  private readonly rangeEl: HTMLDivElement;
  private readonly markerEl: HTMLDivElement;
  private readonly activeEl: HTMLDivElement;
  private readonly editor: HTMLInputElement;

  private cells: PooledCell[] = [];
  private colHeaders: PooledHeader[] = [];
  private rowHeaders: PooledHeader[] = [];
  private renderedRect: CellRect = EMPTY_RECT;
  private rafHandle = 0;
  private resizeObserver?: ResizeObserver;

  private sel: Selection = { anchor: { row: 0, col: 0 }, active: { row: 0, col: 0 } };
  private editing = false;
  private replaceMode = false;
  private suppressBlur = false;
  private dragging = false;

  stats: RenderStats = { rect: EMPTY_RECT, renderedCells: 0 };

  constructor(
    root: HTMLElement,
    private source: GridDataSource,
    options: GridOptions = {},
    private readonly events: GridEvents = {},
  ) {
    this.geo = options.geometry ?? DEFAULT_GEOMETRY;
    this.buffer = options.buffer ?? 3;
    this.isMac = options.isMac ?? isMacPlatform(navigator);
    this.scrollBehavior =
      options.scrollBehavior ??
      (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth");

    root.classList.add("ssgrid");
    root.style.setProperty("--ss-row-h", `${this.geo.rowHeight}px`);
    root.style.setProperty("--ss-col-w", `${this.geo.colWidth}px`);
    root.style.setProperty("--ss-hdr-h", `${this.geo.headerHeight}px`);
    root.style.setProperty("--ss-rowhdr-w", `${this.geo.rowHeaderWidth}px`);

    const corner = el("div", "ssgrid-corner");
    const colHdr = el("div", "ssgrid-colhdr");
    this.colTrack = el("div", "ssgrid-track");
    colHdr.appendChild(this.colTrack);
    const rowHdr = el("div", "ssgrid-rowhdr");
    this.rowTrack = el("div", "ssgrid-track");
    rowHdr.appendChild(this.rowTrack);

    this.viewport = el("div", "ssgrid-viewport");
    this.viewport.tabIndex = 0;
    this.viewport.setAttribute("role", "grid");
    this.viewport.setAttribute("aria-label", "spreadsheet");
    this.content = el("div", "ssgrid-content");
    this.cellLayer = el("div", "ssgrid-cells");
    this.rangeEl = el("div", "ssgrid-range");
    this.markerEl = el("div", "ssgrid-marker");
    this.activeEl = el("div", "ssgrid-active");
    this.editor = document.createElement("input");
    this.editor.className = "ssgrid-editor";
    this.editor.spellcheck = false;
    this.editor.autocomplete = "off";
    this.editor.setAttribute("aria-label", "cell editor");
    this.content.append(this.cellLayer, this.rangeEl, this.markerEl, this.activeEl, this.editor);
    this.viewport.appendChild(this.content);
    root.replaceChildren(corner, colHdr, rowHdr, this.viewport);

    this.bindEvents(colHdr, rowHdr);
    this.applySheetSize();
    this.renderSelection();
    this.scheduleRender();
  }

  // ------------------------------------------------------------ public API

  get selection(): Selection {
    return { anchor: { ...this.sel.anchor }, active: { ...this.sel.active } };
  }

  get activeCell(): CellPos {
    return { ...this.sel.active };
  }

  get isEditing(): boolean {
    return this.editing;
  }

  get editorText(): string {
    return this.editor.value;
  }

  /** Swap the underlying sheet (e.g. when the user switches tabs). */
  setSource(source: GridDataSource): void {
    if (this.editing) this.cancelEdit();
    this.source = source;
    this.applySheetSize();
    this.setMarker(null);
    this.viewport.scrollTo({ top: 0, left: 0, behavior: "auto" });
    this.setSelection({ row: 0, col: 0 }, false, false);
    this.forceRender();
  }

  /** Selects a rectangle; the cursor goes to its top-left. */
  selectRect(rect: CellRect, scroll = true): void {
    const active = clampPos({ row: rect.r1, col: rect.c1 }, this.source.rows, this.source.cols);
    const anchor = clampPos({ row: rect.r2, col: rect.c2 }, this.source.rows, this.source.cols);
    this.sel = { anchor, active };
    if (scroll) this.reveal(active);
    this.renderSelection();
    this.events.onSelectionChange?.(this.selection);
  }

  /** Dashed outline marking the clipboard source (like "marching ants"). */
  setMarker(rect: CellRect | null): void {
    if (!rect) {
      this.markerEl.style.display = "none";
      return;
    }
    const tl = cellBox({ row: rect.r1, col: rect.c1 }, this.geo);
    this.markerEl.style.display = "block";
    this.markerEl.style.transform = `translate(${tl.left}px, ${tl.top}px)`;
    this.markerEl.style.width = `${(rect.c2 - rect.c1 + 1) * this.geo.colWidth}px`;
    this.markerEl.style.height = `${(rect.r2 - rect.r1 + 1) * this.geo.rowHeight}px`;
  }

  focus(): void {
    // Never steal focus from an open modal dialog or the in-cell editor.
    if (this.editing || document.querySelector("dialog[open]")) return;
    this.viewport.focus({ preventScroll: true });
  }

  /** Moves the cursor. `extend` keeps the anchor (range selection). */
  setSelection(pos: CellPos, extend = false, scroll = true): void {
    const active = clampPos(pos, this.source.rows, this.source.cols);
    const next: Selection = { anchor: extend ? this.sel.anchor : active, active };
    const changed = !samePos(next.active, this.sel.active) || !samePos(next.anchor, this.sel.anchor);
    this.sel = next;
    if (scroll) this.reveal(active);
    this.renderSelection();
    if (changed) this.events.onSelectionChange?.(this.selection);
  }

  /** Starts in-cell editing of the active cell. */
  beginEdit(replace: boolean, initialText = ""): void {
    if (this.editing) return;
    this.editing = true;
    this.replaceMode = replace;
    this.reveal(this.sel.active, "auto");
    this.positionEditor();
    this.editor.value = replace ? initialText : this.source.input(this.sel.active);
    this.editor.style.display = "block";
    this.editor.focus();
    const end = this.editor.value.length;
    this.editor.setSelectionRange(end, end);
    this.events.onEditingChange?.(this.editor.value);
  }

  /** Commits the in-cell editor through the Rust workbook. */
  commitEdit(move: "down" | "up" | "right" | "left" | "none" = "none"): boolean {
    if (!this.editing) return true;
    const text = this.editor.value;
    const result = this.source.edit(this.sel.active, text);
    if (!result.ok) {
      // Keep the editor open so the user can fix the formula (Esc cancels).
      this.reportResult(result, `edit ${cellRef(this.sel.active)}`);
      this.editor.focus();
      return false;
    }
    this.hideEditor();
    this.focus();
    this.forceRender();
    this.reportResult(result, `edit ${cellRef(this.sel.active)}`);
    const d = moveDelta(move);
    if (d.dRow || d.dCol) this.moveBy(d.dRow, d.dCol, false);
    return true;
  }

  cancelEdit(): void {
    if (!this.editing) return;
    this.hideEditor();
    this.focus();
  }

  /** Mirrors formula-bar typing into the in-cell editor (starts it if needed). */
  setEditorText(text: string): void {
    if (!this.editing) {
      this.editing = true;
      this.replaceMode = false;
      this.positionEditor();
      this.editor.style.display = "block";
    }
    this.editor.value = text;
    this.sizeEditor();
  }

  /** Commits text coming from the formula bar for the active cell. */
  commitExternalEdit(text: string, move: "down" | "none" = "down"): boolean {
    if (this.editing) {
      this.editor.value = text;
      return this.commitEdit(move);
    }
    const result = this.source.edit(this.sel.active, text);
    this.forceRender();
    this.reportResult(result, `edit ${cellRef(this.sel.active)}`);
    if (result.ok && move === "down") this.moveBy(1, 0, false);
    return result.ok;
  }

  /** Clears the selected rectangle. */
  clearSelection(): void {
    const rect = selectionRect(this.sel);
    const result = this.source.clearRect(rect);
    this.forceRender();
    this.reportResult(result, `clear ${rectCellCount(rect)} cell(s)`);
  }

  /** Re-reads the visible window from the workbook and repaints. */
  forceRender(): void {
    this.renderedRect = EMPTY_RECT;
    this.render();
  }

  destroy(): void {
    cancelAnimationFrame(this.rafHandle);
    this.resizeObserver?.disconnect();
  }

  // --------------------------------------------------------------- events

  private bindEvents(colHdr: HTMLDivElement, rowHdr: HTMLDivElement): void {
    this.viewport.addEventListener("scroll", () => this.scheduleRender(), { passive: true });
    this.resizeObserver = new ResizeObserver(() => this.scheduleRender());
    this.resizeObserver.observe(this.viewport);

    this.viewport.addEventListener("keydown", (e) => this.onGridKey(e));
    this.viewport.addEventListener("pointerdown", (e) => this.onPointerDown(e));
    this.viewport.addEventListener("pointermove", (e) => this.onPointerMove(e));
    this.viewport.addEventListener("pointerup", () => (this.dragging = false));
    this.viewport.addEventListener("pointercancel", () => (this.dragging = false));
    this.viewport.addEventListener("dblclick", (e) => {
      if (e.target === this.editor) return;
      this.beginEdit(false);
    });
    this.viewport.addEventListener("contextmenu", (e) => {
      if (e.target === this.editor) return;
      e.preventDefault();
      const pos = this.posFromEvent(e);
      if (this.editing && !this.commitEdit("none")) return;
      if (!rectContains(selectionRect(this.sel), pos)) this.setSelection(pos, false, false);
      this.focus();
      this.events.onContextMenu?.(e.clientX, e.clientY);
    });

    this.editor.addEventListener("keydown", (e) => this.onEditorKey(e));
    this.editor.addEventListener("input", () => {
      this.sizeEditor();
      this.events.onEditingChange?.(this.editor.value);
    });
    this.editor.addEventListener("blur", () => {
      if (this.suppressBlur || !this.editing) return;
      // Focus moved elsewhere (e.g. formula bar): commit what was typed.
      this.commitEdit("none");
    });

    colHdr.addEventListener("pointerdown", (e) => {
      const x = e.clientX - this.colTrack.getBoundingClientRect().left;
      const col = clampPos({ row: 0, col: Math.floor(x / this.geo.colWidth) }, 1, this.source.cols).col;
      if (this.editing) this.commitEdit("none");
      // Whole column: cursor at the top so typing goes to row 1.
      this.sel = { anchor: { row: this.source.rows - 1, col }, active: { row: 0, col } };
      this.renderSelection();
      this.events.onSelectionChange?.(this.selection);
      this.focus();
    });
    rowHdr.addEventListener("pointerdown", (e) => {
      const y = e.clientY - this.rowTrack.getBoundingClientRect().top;
      const row = clampPos({ row: Math.floor(y / this.geo.rowHeight), col: 0 }, this.source.rows, 1).row;
      if (this.editing) this.commitEdit("none");
      this.sel = { anchor: { row, col: this.source.cols - 1 }, active: { row, col: 0 } };
      this.renderSelection();
      this.events.onSelectionChange?.(this.selection);
      this.focus();
    });
  }

  private onPointerDown(e: PointerEvent): void {
    if (e.button !== 0 || e.target === this.editor) return;
    const pos = this.posFromEvent(e);
    if (this.editing) {
      if (!this.commitEdit("none")) return; // invalid formula: stay in the editor
    }
    this.setSelection(pos, e.shiftKey, false);
    this.dragging = true;
    this.viewport.setPointerCapture(e.pointerId);
    this.focus();
    e.preventDefault();
  }

  private onPointerMove(e: PointerEvent): void {
    if (!this.dragging) return;
    const pos = this.posFromEvent(e);
    if (!samePos(pos, this.sel.active)) this.setSelection(pos, true, true);
  }

  private posFromEvent(e: PointerEvent): CellPos {
    const rect = this.content.getBoundingClientRect();
    return cellAtPoint(e.clientX - rect.left, e.clientY - rect.top, this.geo, this.source.rows, this.source.cols);
  }

  private onGridKey(e: KeyboardEvent): void {
    // Keys typed into the editor bubble up here; they are handled by onEditorKey.
    if (this.editing || e.target === this.editor) return;
    const action = keyToGridAction(e, this.isMac);
    if (!action) return;
    e.preventDefault();
    this.runAction(action);
  }

  private runAction(action: GridAction): void {
    switch (action.type) {
      case "move":
        this.moveBy(action.dRow, action.dCol, action.extend);
        break;
      case "page": {
        const rowsPerPage = Math.max(1, Math.floor(this.viewport.clientHeight / this.geo.rowHeight) - 1);
        this.moveBy(action.dir * rowsPerPage, 0, action.extend);
        break;
      }
      case "jump": {
        const a = this.sel.active;
        const last = { row: this.source.rows - 1, col: this.source.cols - 1 };
        const target: Record<typeof action.to, CellPos> = {
          rowStart: { row: a.row, col: 0 },
          rowEnd: { row: a.row, col: last.col },
          sheetStart: { row: 0, col: 0 },
          sheetEnd: last,
          up: { row: 0, col: a.col },
          down: { row: last.row, col: a.col },
          left: { row: a.row, col: 0 },
          right: { row: a.row, col: last.col },
        };
        this.setSelection(target[action.to], action.extend);
        break;
      }
      case "edit":
        this.beginEdit(false);
        break;
      case "type":
        this.beginEdit(true, action.text);
        break;
      case "clear":
        this.clearSelection();
        break;
      case "copy":
      case "cut":
      case "paste":
      case "undo":
      case "redo":
      case "goto":
        this.events.onCommand?.(action.type);
        break;
      case "selectAll":
        this.sel = { anchor: { row: 0, col: 0 }, active: { row: this.source.rows - 1, col: this.source.cols - 1 } };
        this.renderSelection();
        this.events.onSelectionChange?.(this.selection);
        break;
    }
  }

  private onEditorKey(e: KeyboardEvent): void {
    e.stopPropagation();
    const action = keyToEditorAction(e, this.isMac, this.replaceMode);
    if (!action) return;
    e.preventDefault();
    if (action.type === "cancel") this.cancelEdit();
    else this.commitEdit(action.move);
  }

  private moveBy(dRow: number, dCol: number, extend: boolean): void {
    const a = this.sel.active;
    this.setSelection({ row: a.row + dRow, col: a.col + dCol }, extend);
  }

  private reportResult(result: EditResult, what: string): void {
    this.events.onDataChange?.(result, what);
    if (result.ok) {
      const sleep = result.sleepSeconds ? ` (SLEEP ${result.sleepSeconds}s requested, not performed)` : "";
      this.events.onStatus?.(`${what}: ok${sleep}`, "ok");
    } else {
      this.events.onStatus?.(`${what}: ${result.status}`, "error");
    }
  }

  // -------------------------------------------------------------- editor

  private positionEditor(): void {
    const box = cellBox(this.sel.active, this.geo);
    this.editor.style.transform = `translate(${box.left}px, ${box.top}px)`;
    this.editor.style.height = `${box.height}px`;
    this.sizeEditor();
  }

  private sizeEditor(): void {
    const minWidth = this.geo.colWidth;
    const wanted = Math.max(minWidth, this.editor.value.length * 8 + 20);
    const maxWidth = (this.source.cols - this.sel.active.col) * this.geo.colWidth;
    this.editor.style.width = `${Math.min(wanted, Math.max(minWidth, maxWidth))}px`;
  }

  private hideEditor(): void {
    this.suppressBlur = true;
    this.editing = false;
    this.editor.style.display = "none";
    this.editor.value = "";
    this.suppressBlur = false;
    this.events.onEditingChange?.(null);
  }

  // ----------------------------------------------------------- rendering

  private applySheetSize(): void {
    this.content.style.width = `${this.source.cols * this.geo.colWidth}px`;
    this.content.style.height = `${this.source.rows * this.geo.rowHeight}px`;
  }

  private currentViewport(): Viewport {
    return {
      scrollTop: this.viewport.scrollTop,
      scrollLeft: this.viewport.scrollLeft,
      width: this.viewport.clientWidth,
      height: this.viewport.clientHeight,
    };
  }

  private reveal(pos: CellPos, behavior: ScrollBehavior = this.scrollBehavior): void {
    const target = scrollToReveal(pos, this.currentViewport(), this.geo);
    if (target) this.viewport.scrollTo({ top: target.scrollTop, left: target.scrollLeft, behavior });
  }

  private scheduleRender(): void {
    if (this.rafHandle) return;
    this.rafHandle = requestAnimationFrame(() => {
      this.rafHandle = 0;
      this.render();
    });
  }

  private render(): void {
    const vp = this.currentViewport();
    // Headers follow the scroll position (they live outside the scroller).
    this.colTrack.style.transform = `translateX(${-vp.scrollLeft}px)`;
    this.rowTrack.style.transform = `translateY(${-vp.scrollTop}px)`;

    const rect = visibleRange(vp, this.geo, this.source.rows, this.source.cols, this.buffer);
    const r = this.renderedRect;
    if (rect.r1 === r.r1 && rect.r2 === r.r2 && rect.c1 === r.c1 && rect.c2 === r.c2) return;
    this.renderedRect = rect;

    const data = new Map<string, CellSnapshot>();
    for (const cell of this.source.range(rect)) data.set(`${cell.row},${cell.col}`, cell);

    const needed = rect.r2 < rect.r1 ? 0 : rectCellCount(rect);
    this.growPool(needed);
    let i = 0;
    for (let row = rect.r1; row <= rect.r2; row++) {
      for (let col = rect.c1; col <= rect.c2; col++) {
        const pooled = this.cells[i++];
        if (pooled.row !== row || pooled.col !== col) {
          pooled.row = row;
          pooled.col = col;
          pooled.el.style.transform = `translate(${col * this.geo.colWidth}px, ${row * this.geo.rowHeight}px)`;
        }
        const snap = data.get(`${row},${col}`);
        const text = snap ? snap.display : "";
        const cls = snap ? cellClass(snap) : "ssgrid-cell";
        const css = snap ? formatToCss(snap.format) : "";
        if (pooled.text !== text) {
          pooled.text = text;
          pooled.el.textContent = text;
        }
        if (pooled.cls !== cls) {
          pooled.cls = cls;
          pooled.el.className = cls;
        }
        if (pooled.css !== css) {
          pooled.css = css;
          // cssText replaces the transform too, so re-apply the position.
          pooled.el.style.cssText = css;
          pooled.el.style.transform = `translate(${col * this.geo.colWidth}px, ${row * this.geo.rowHeight}px)`;
        }
      }
    }
    for (; i < this.cells.length; i++) {
      if (this.cells[i].row !== -1) {
        this.cells[i].row = -1;
        this.cells[i].el.style.display = "none";
      }
    }

    this.renderHeaders(rect);
    this.stats = { rect, renderedCells: needed };
  }

  private growPool(needed: number): void {
    while (this.cells.length < needed) {
      const cell = el("div", "ssgrid-cell");
      this.cellLayer.appendChild(cell);
      this.cells.push({ el: cell, row: -2, col: -2, text: "", cls: "ssgrid-cell", css: "" });
    }
    for (let i = 0; i < needed; i++) {
      if (this.cells[i].row === -1) this.cells[i].el.style.display = "";
    }
  }

  private renderHeaders(rect: CellRect): void {
    const selRect = selectionRect(this.sel);
    syncHeaders(
      this.colHeaders,
      this.colTrack,
      rect.c1,
      rect.c2,
      (i) => columnName(i),
      (i) => i * this.geo.colWidth,
      "x",
      (i) => i >= selRect.c1 && i <= selRect.c2,
    );
    syncHeaders(
      this.rowHeaders,
      this.rowTrack,
      rect.r1,
      rect.r2,
      (i) => String(i + 1),
      (i) => i * this.geo.rowHeight,
      "y",
      (i) => i >= selRect.r1 && i <= selRect.r2,
    );
  }

  private renderSelection(): void {
    const rect = selectionRect(this.sel);
    const active = cellBox(this.sel.active, this.geo);
    this.activeEl.style.transform = `translate(${active.left}px, ${active.top}px)`;
    if (isSingleCell(this.sel)) {
      this.rangeEl.style.display = "none";
    } else {
      const tl = cellBox({ row: rect.r1, col: rect.c1 }, this.geo);
      this.rangeEl.style.display = "block";
      this.rangeEl.style.transform = `translate(${tl.left}px, ${tl.top}px)`;
      this.rangeEl.style.width = `${(rect.c2 - rect.c1 + 1) * this.geo.colWidth}px`;
      this.rangeEl.style.height = `${(rect.r2 - rect.r1 + 1) * this.geo.rowHeight}px`;
    }
    this.viewport.setAttribute("aria-activedescendant", cellRef(this.sel.active));
    if (this.renderedRect.r2 >= this.renderedRect.r1) this.renderHeaders(this.renderedRect);
  }
}

// ------------------------------------------------------------------ utils

function el(tag: "div", className: string): HTMLDivElement {
  const node = document.createElement(tag);
  node.className = className;
  return node;
}

function cellClass(snap: CellSnapshot): string {
  let cls = "ssgrid-cell";
  if (snap.isError) cls += " is-error";
  else if (snap.isString) cls += " is-string";
  else if (snap.display !== "") cls += " is-number";
  if (snap.hasFormula) cls += " has-formula";
  return cls;
}

function syncHeaders(
  pool: PooledHeader[],
  track: HTMLDivElement,
  first: number,
  last: number,
  label: (i: number) => string,
  offset: (i: number) => number,
  axis: "x" | "y",
  highlighted: (i: number) => boolean,
): void {
  const needed = last < first ? 0 : last - first + 1;
  while (pool.length < needed) {
    const node = el("div", "ssgrid-hdr");
    track.appendChild(node);
    pool.push({ el: node, index: -2, text: "", hl: false });
  }
  for (let i = 0; i < pool.length; i++) {
    const h = pool[i];
    if (i >= needed) {
      if (h.index !== -1) {
        h.index = -1;
        h.el.style.display = "none";
      }
      continue;
    }
    const index = first + i;
    if (h.index === -1) h.el.style.display = "";
    if (h.index !== index) {
      h.index = index;
      const o = offset(index);
      h.el.style.transform = axis === "x" ? `translateX(${o}px)` : `translateY(${o}px)`;
    }
    const text = label(index);
    if (h.text !== text) {
      h.text = text;
      h.el.textContent = text;
    }
    const hl = highlighted(index);
    if (h.hl !== hl) {
      h.hl = hl;
      h.el.classList.toggle("is-selected", hl);
    }
  }
}
