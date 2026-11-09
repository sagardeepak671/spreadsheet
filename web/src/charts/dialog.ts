/**
 * Insert / edit chart dialog with a live preview. The preview and the final
 * chart both use `ChartData` computed by the Rust core from the cells.
 */

import type { ChartData, ChartKind, ChartSpec, Rect } from "../bridge";
import { cellRef, parseRef, type CellRect } from "../grid/model";
import { chartTheme } from "../theme";
import { renderChartSvg } from "./render";

export type ChartDraft = Omit<ChartSpec, "id"> & { id?: number };

export interface ChartDialogElements {
  dialog: HTMLDialogElement;
  form: HTMLFormElement;
  title: HTMLInputElement;
  kind: HTMLSelectElement;
  rangeStart: HTMLInputElement;
  rangeEnd: HTMLInputElement;
  xLabel: HTMLInputElement;
  yLabel: HTMLInputElement;
  seriesInRows: HTMLInputElement;
  headerLabels: HTMLInputElement;
  firstSeriesIsX: HTMLInputElement;
  preview: HTMLElement;
  warnings: HTMLElement;
  error: HTMLElement;
  submit: HTMLButtonElement;
  heading: HTMLElement;
}

export class ChartDialog {
  private onSubmit: ((draft: ChartDraft) => void) | null = null;

  constructor(
    private readonly els: ChartDialogElements,
    /** Computes live data for a draft (Rust). */
    private readonly dataFor: (draft: ChartDraft) => ChartData,
  ) {
    for (const input of [els.title, els.kind, els.rangeStart, els.rangeEnd, els.xLabel, els.yLabel, els.seriesInRows, els.headerLabels, els.firstSeriesIsX]) {
      input.addEventListener("input", () => this.refreshPreview());
      input.addEventListener("change", () => this.refreshPreview());
    }
    els.form.addEventListener("submit", (e) => {
      e.preventDefault();
      const draft = this.draft();
      if (!draft) return;
      els.dialog.close();
      this.onSubmit?.(draft);
    });
  }

  /** Opens for a new chart on `rect` (or for editing `existing`). */
  open(rect: CellRect, existing: ChartSpec | null, onSubmit: (draft: ChartDraft) => void): void {
    const { els } = this;
    this.onSubmit = onSubmit;
    const spec: ChartDraft = existing ?? {
      kind: "line",
      title: "",
      xLabel: "",
      yLabel: "",
      range: { row1: rect.r1, col1: rect.c1, row2: rect.r2, col2: rect.c2 },
      seriesInRows: false,
      headerLabels: false,
      firstSeriesIsX: false,
    };
    els.heading.textContent = existing ? "Edit chart" : "Insert chart";
    els.submit.textContent = existing ? "Save" : "Insert";
    els.title.value = spec.title;
    els.kind.value = spec.kind;
    els.rangeStart.value = cellRef({ row: spec.range.row1, col: spec.range.col1 });
    els.rangeEnd.value = cellRef({ row: spec.range.row2, col: spec.range.col2 });
    els.xLabel.value = spec.xLabel;
    els.yLabel.value = spec.yLabel;
    els.seriesInRows.checked = spec.seriesInRows;
    els.headerLabels.checked = spec.headerLabels;
    els.firstSeriesIsX.checked = spec.firstSeriesIsX;
    if (existing) (els.form as HTMLFormElement & { dataset: DOMStringMap }).dataset.id = String(existing.id);
    else delete els.form.dataset.id;
    this.refreshPreview();
    if (!els.dialog.open) els.dialog.showModal();
    els.title.focus();
  }

  /** Current form state as a chart definition, or null if invalid. */
  draft(): ChartDraft | null {
    const { els } = this;
    const start = parseRef(els.rangeStart.value);
    const end = parseRef(els.rangeEnd.value);
    if (!start || !end) {
      els.error.textContent = "enter a valid range, e.g. A1 and C12";
      return null;
    }
    els.error.textContent = "";
    const range: Rect = {
      row1: Math.min(start.row, end.row),
      col1: Math.min(start.col, end.col),
      row2: Math.max(start.row, end.row),
      col2: Math.max(start.col, end.col),
    };
    const draft: ChartDraft = {
      kind: els.kind.value as ChartKind,
      title: els.title.value.trim(),
      xLabel: els.xLabel.value.trim(),
      yLabel: els.yLabel.value.trim(),
      range,
      seriesInRows: els.seriesInRows.checked,
      headerLabels: els.headerLabels.checked,
      firstSeriesIsX: els.firstSeriesIsX.checked,
    };
    if (els.form.dataset.id) draft.id = Number(els.form.dataset.id);
    return draft;
  }

  refreshPreview(): void {
    const draft = this.draft();
    if (!draft) {
      this.els.preview.innerHTML = "";
      this.els.warnings.textContent = "";
      return;
    }
    try {
      const data = this.dataFor(draft);
      this.els.preview.innerHTML = renderChartSvg(data, { width: 480, height: 280, theme: chartTheme() });
      this.els.warnings.textContent = data.warnings.join(" · ");
    } catch (err) {
      this.els.preview.innerHTML = "";
      this.els.warnings.textContent = (err as Error).message;
    }
  }
}
