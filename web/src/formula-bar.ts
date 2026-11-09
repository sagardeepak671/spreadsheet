/**
 * Name box + formula input + computed value. Everything displayed is read
 * from the Rust workbook via the grid's data source; the bar itself holds no
 * spreadsheet state.
 */

import type { SpreadsheetGrid } from "./grid/grid";
import { cellRef, parseRef, type CellPos } from "./grid/model";
import type { GridDataSource } from "./grid/sheet-adapter";

export interface FormulaBarElements {
  nameBox: HTMLInputElement;
  formulaInput: HTMLInputElement;
  valueOut: HTMLElement;
}

export class FormulaBar {
  private source: GridDataSource;
  private editingInBar = false;

  constructor(
    private readonly els: FormulaBarElements,
    private readonly grid: SpreadsheetGrid,
    source: GridDataSource,
    /** Gets first shot at Enter; return true if the text was a command (e.g. `SORT(...)`). */
    private readonly commandHandler: (text: string) => boolean = () => false,
  ) {
    this.source = source;

    els.nameBox.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        this.goto(els.nameBox.value);
      } else if (e.key === "Escape") {
        e.preventDefault();
        this.refresh();
        grid.focus();
      }
    });
    els.nameBox.addEventListener("focus", () => els.nameBox.select());

    els.formulaInput.addEventListener("focus", () => {
      this.editingInBar = true;
    });
    els.formulaInput.addEventListener("input", () => {
      // Mirror into the in-cell editor so both views show the same draft.
      grid.setEditorText(els.formulaInput.value);
    });
    els.formulaInput.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        this.editingInBar = false;
        if (this.commandHandler(els.formulaInput.value)) {
          grid.cancelEdit();
          this.refresh();
          grid.focus();
        } else if (grid.commitExternalEdit(els.formulaInput.value, e.shiftKey ? "none" : "down")) {
          grid.focus();
        }
      } else if (e.key === "Tab") {
        e.preventDefault();
        this.editingInBar = false;
        if (grid.commitExternalEdit(els.formulaInput.value, "none")) grid.focus();
      } else if (e.key === "Escape") {
        e.preventDefault();
        this.editingInBar = false;
        grid.cancelEdit();
        this.refresh();
        grid.focus();
      }
    });
    els.formulaInput.addEventListener("blur", () => {
      this.editingInBar = false;
    });
  }

  setSource(source: GridDataSource): void {
    this.source = source;
    this.refresh();
  }

  /** Re-reads name / input / value of the active cell from the workbook. */
  refresh(pos: CellPos = this.grid.activeCell): void {
    this.els.nameBox.value = cellRef(pos);
    if (!this.editingInBar && !this.grid.isEditing) {
      this.els.formulaInput.value = this.source.input(pos);
    }
    this.els.valueOut.textContent = this.source.display(pos);
  }

  /** Live draft from the in-cell editor (null = editing ended). */
  mirrorEditor(text: string | null): void {
    if (text === null) {
      this.refresh();
    } else if (!this.editingInBar) {
      this.els.formulaInput.value = text;
    }
  }

  focusNameBox(): void {
    this.els.nameBox.focus();
  }

  goto(text: string): boolean {
    const pos = parseRef(text);
    if (!pos || pos.row >= this.source.rows || pos.col >= this.source.cols) {
      this.els.nameBox.classList.add("is-invalid");
      setTimeout(() => this.els.nameBox.classList.remove("is-invalid"), 600);
      return false;
    }
    this.grid.setSelection(pos);
    this.grid.focus();
    return true;
  }
}
