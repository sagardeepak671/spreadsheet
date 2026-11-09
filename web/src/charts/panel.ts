/**
 * Side panel showing the charts of the current sheet. Every repaint asks the
 * Rust core for fresh `ChartData`, so charts follow edits and formulas.
 */

import type { ChartData, ChartSpec } from "../bridge";
import { cellRef } from "../grid/model";
import { icon } from "../icons";
import { chartTheme } from "../theme";
import { renderChartSvg } from "./render";

export interface ChartPanelHandlers {
  onEdit(spec: ChartSpec): void;
  onDelete(spec: ChartSpec): void;
  onSelectRange(spec: ChartSpec): void;
  onClose(): void;
}

export class ChartPanel {
  private lastKey = "";

  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: ChartPanelHandlers,
  ) {}

  get isOpen(): boolean {
    return !this.root.hidden;
  }

  open(): void {
    this.root.hidden = false;
  }

  close(): void {
    this.root.hidden = true;
    this.handlers.onClose();
  }

  /** Forces the next render to repaint (e.g. after a theme change). */
  invalidate(): void {
    this.lastKey = "";
  }

  /** Re-renders all charts from live data. */
  render(charts: ChartSpec[], dataFor: (spec: ChartSpec) => ChartData): void {
    const list = document.createElement("div");
    list.className = "chart-list";
    for (const spec of charts) {
      const card = document.createElement("section");
      card.className = "chart-card";
      card.dataset.chartId = String(spec.id);

      const head = document.createElement("header");
      const title = document.createElement("span");
      title.className = "chart-card-title";
      title.textContent = spec.title || `${spec.kind} chart`;
      const range = document.createElement("button");
      range.type = "button";
      range.className = "chart-card-range";
      range.textContent = `${cellRef({ row: spec.range.row1, col: spec.range.col1 })}:${cellRef({ row: spec.range.row2, col: spec.range.col2 })}`;
      range.title = "select the chart's data range";
      range.addEventListener("click", () => this.handlers.onSelectRange(spec));
      const edit = document.createElement("button");
      edit.type = "button";
      edit.className = "btn-icon";
      edit.title = "Edit chart";
      edit.setAttribute("aria-label", "Edit");
      edit.innerHTML = icon("edit");
      edit.addEventListener("click", () => this.handlers.onEdit(spec));
      const del = document.createElement("button");
      del.type = "button";
      del.className = "btn-icon danger";
      del.title = "Delete chart";
      del.setAttribute("aria-label", "Delete");
      del.innerHTML = icon("trash");
      del.addEventListener("click", () => this.handlers.onDelete(spec));
      head.append(title, range, edit, del);

      const body = document.createElement("div");
      body.className = "chart-card-body";
      try {
        const data = dataFor(spec);
        body.innerHTML = renderChartSvg(data, { width: 440, height: 280, theme: chartTheme() });
        if (data.warnings.length) {
          const warn = document.createElement("p");
          warn.className = "chart-warning";
          warn.textContent = data.warnings.join(" · ");
          body.appendChild(warn);
        }
      } catch (err) {
        body.textContent = (err as Error).message;
      }
      card.append(head, body);
      list.appendChild(card);
    }
    if (charts.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = "No charts on this sheet yet. Select a range and use Insert chart.";
      list.appendChild(empty);
    }
    const key = list.innerHTML;
    if (key === this.lastKey) return;
    this.lastKey = key;
    const content = this.root.querySelector(".chart-panel-content");
    if (content) content.replaceChildren(list);
  }
}
