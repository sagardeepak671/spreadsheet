/**
 * Clipboard with the desktop semantics:
 *
 * * **cut + paste** moves the cells (`Workbook::move_cells`): formulas that
 *   referenced them are rewritten to the new position;
 * * **copy + paste** pastes the current *values* (`Workbook::copy_values`),
 *   formulas are not copied;
 * * text copied from *another* application is pasted cell by cell with the
 *   normal editing rules (`paste_text_block`).
 *
 * The system clipboard receives the displayed values as tab-separated text
 * (so you can paste into other apps); the in-app source range is remembered
 * here. On paste, if the system clipboard still holds exactly what we wrote
 * (or cannot be read), the in-app semantics apply; otherwise the foreign
 * text is pasted.
 */

import type { EditResult } from "./bridge";
import { parseClipboardText, rectCellCount, toClipboardText, type CellPos, type CellRect } from "./grid/model";
import type { SheetAdapter } from "./grid/sheet-adapter";

export type ClipMode = "cut" | "copy";

export interface ClipSource {
  mode: ClipMode;
  sheet: number;
  rect: CellRect;
  /** Tab-separated display text written to the system clipboard. */
  text: string;
}

export interface SystemClipboard {
  writeText(text: string): Promise<void>;
  readText(): Promise<string>;
}

/** `navigator.clipboard` if present; otherwise a no-op that makes reads fail. */
export function browserClipboard(): SystemClipboard {
  const clip = typeof navigator !== "undefined" ? navigator.clipboard : undefined;
  return {
    writeText: (text) => (clip ? clip.writeText(text) : Promise.reject(new Error("no clipboard"))),
    readText: () => (clip ? clip.readText() : Promise.reject(new Error("no clipboard"))),
  };
}

export interface PasteOutcome {
  result: EditResult;
  /** Rectangle that received the paste (to select it). */
  rect: CellRect;
  /** Which semantics were used. */
  kind: "move" | "values" | "text";
}

export class ClipboardController {
  private source: ClipSource | null = null;

  constructor(private readonly system: SystemClipboard = browserClipboard()) {}

  get current(): ClipSource | null {
    return this.source;
  }

  clear(): void {
    this.source = null;
  }

  /** Remembers the range and mirrors its display text to the system clipboard. */
  async copy(adapter: SheetAdapter, rect: CellRect, mode: ClipMode): Promise<number> {
    const text = toClipboardText(adapter.displayBlock(rect));
    this.source = { mode, sheet: adapter.sheet, rect, text };
    try {
      await this.system.writeText(text);
    } catch {
      /* insecure context / permission denied: in-app paste still works */
    }
    return rectCellCount(rect);
  }

  /**
   * Pastes at `target` (top-left). Returns null when there is nothing to
   * paste at all.
   */
  async paste(adapter: SheetAdapter, target: CellPos): Promise<PasteOutcome | null> {
    let external = "";
    try {
      external = await this.system.readText();
    } catch {
      external = "";
    }
    const src = this.source;
    const useInternal = src !== null && (external === "" || external === src.text);

    if (useInternal && src) {
      const rows = src.rect.r2 - src.rect.r1;
      const cols = src.rect.c2 - src.rect.c1;
      const rect: CellRect = {
        r1: target.row,
        c1: target.col,
        r2: Math.min(adapter.rows - 1, target.row + rows),
        c2: Math.min(adapter.cols - 1, target.col + cols),
      };
      if (src.sheet !== adapter.sheet) {
        // Cross-sheet: the engine works per sheet, so paste the values as text.
        const block = parseClipboardText(src.text);
        return { result: adapter.pasteText(target, block), rect, kind: "text" };
      }
      if (src.mode === "cut") {
        const result = adapter.moveCells(src.rect, target);
        this.source = null; // a cut can be pasted once, like the desktop
        return { result, rect, kind: "move" };
      }
      return { result: adapter.copyValues(src.rect, target), rect, kind: "values" };
    }

    if (external === "") return null;
    const block = parseClipboardText(external);
    const rect: CellRect = {
      r1: target.row,
      c1: target.col,
      r2: Math.min(adapter.rows - 1, target.row + block.length - 1),
      c2: Math.min(adapter.cols - 1, target.col + Math.max(...block.map((l) => l.length)) - 1),
    };
    return { result: adapter.pasteText(target, block), rect, kind: "text" };
  }
}
