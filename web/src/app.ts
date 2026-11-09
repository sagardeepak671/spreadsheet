/**
 * Application controller: owns the workbook, the grid, the formula bar, the
 * sheet tabs, the clipboard, the dialogs, the file flow and the auto-save.
 * All spreadsheet semantics still come from the Rust core; this file only
 * routes UI events to it and refreshes the views from it.
 */

import { AutoSaver, type AutosaveState } from "./autosave";
import { bridgeVersion, SpreadsheetWorkbook, type ChartSpec, type EditResult, type FormatPatch, type HistoryOutcome } from "./bridge";
import { ChartDialog, type ChartDraft } from "./charts/dialog";
import { ChartPanel } from "./charts/panel";
import { ClipboardController } from "./clipboard";
import { ContextMenu, MenuBar, PopupMenu, type MenuItem } from "./context-menu";
import { FormatToolbar } from "./format-toolbar";
import { classifyFileName, downloadText, pickFile, readFileText, safeFileName, withExtension } from "./files";
import { FormulaBar } from "./formula-bar";
import { SpreadsheetGrid, type GridCommand } from "./grid/grid";
import { cellRef, isSingleCell, parseRef, rectCellCount, selectionRect, shortcutHelp, type CellRect } from "./grid/model";
import { SheetAdapter } from "./grid/sheet-adapter";
import { HomeScreen } from "./home";
import { icon, type IconName } from "./icons";
import {
  deleteWorkbook,
  loadWorkbook,
  migrateLegacyAutosave,
  newId,
  readIndex,
  renameWorkbook,
  saveWorkbook,
  setCurrent,
} from "./library";
import { SheetTabs } from "./tabs";
import { applyTheme, currentTheme, readTheme, THEMES } from "./theme";

export const DEFAULT_ROWS = 1000;
export const DEFAULT_COLS = 1000;
export const MAX_ROWS = 1000;
export const MAX_COLS = 18278;

const $ = <T extends HTMLElement>(id: string): T => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing #${id}`);
  return node as T;
};

type StatusKind = "ok" | "error" | "info";

export class App {
  private wb: SpreadsheetWorkbook;
  private adapter: SheetAdapter;
  private readonly grid: SpreadsheetGrid;
  private readonly formulaBar: FormulaBar;
  private readonly tabs: SheetTabs;
  private readonly clipboard = new ClipboardController();
  private readonly autosaver: AutoSaver;
  private readonly formatToolbar: FormatToolbar;
  private readonly contextMenu = new ContextMenu();
  private readonly popup = new PopupMenu();
  private readonly chartDialog: ChartDialog;
  private readonly chartPanel: ChartPanel;
  private readonly home: HomeScreen;
  private findMatches: string[] = [];
  private findCursor = -1;
  /** Library id + display name of the open workbook. */
  private currentId: string | null = null;
  private currentName = "Untitled";

  constructor(
    private readonly isMac: boolean,
    private readonly storage: Storage = window.localStorage,
  ) {
    applyTheme(readTheme(storage));
    migrateLegacyAutosave(storage, (json) => {
      const probe = SpreadsheetWorkbook.fromJson(json);
      const n = probe.sheetCount;
      probe.free();
      return n;
    });

    // The grid always exists (hidden behind the home screen until a
    // workbook is open); start with a throwaway workbook.
    this.wb = SpreadsheetWorkbook.create(1, 1);
    this.adapter = new SheetAdapter(this.wb, 0);

    // ---- views
    this.grid = new SpreadsheetGrid(
      $("grid"),
      this.adapter,
      { isMac },
      {
        onSelectionChange: (sel) => {
          this.formulaBar.refresh(sel.active);
          this.updateSelectionInfo();
          this.updateFormatToolbar();
        },
        onDataChange: () => this.changed(),
        onEditingChange: (text) => this.formulaBar.mirrorEditor(text),
        onStatus: (m, k) => this.setStatus(m, k),
        onCommand: (cmd) => void this.runCommand(cmd),
        onContextMenu: (x, y) => this.openContextMenu(x, y),
      },
    );
    this.formatToolbar = new FormatToolbar(
      {
        bold: $("fmt-bold"),
        italic: $("fmt-italic"),
        underline: $("fmt-underline"),
        fontSize: $("fmt-size"),
        color: $("fmt-color"),
        colorClear: $("fmt-color-clear"),
        fill: $("fmt-fill"),
        fillClear: $("fmt-fill-clear"),
        alignLeft: $("fmt-left"),
        alignCenter: $("fmt-center"),
        alignRight: $("fmt-right"),
        clear: $("fmt-clear"),
      },
      (patch) => this.applyFormat(patch),
    );
    this.chartDialog = new ChartDialog(
      {
        dialog: $("dlg-chart"),
        form: $("chart-form"),
        title: $("chart-title"),
        kind: $("chart-kind"),
        rangeStart: $("chart-start"),
        rangeEnd: $("chart-end"),
        xLabel: $("chart-xlabel"),
        yLabel: $("chart-ylabel"),
        seriesInRows: $("chart-rows"),
        headerLabels: $("chart-headers"),
        firstSeriesIsX: $("chart-firstx"),
        preview: $("chart-preview"),
        warnings: $("chart-warnings"),
        error: $("chart-error"),
        submit: $("chart-submit"),
        heading: $("chart-heading"),
      },
      (draft) => this.wb.chartData(this.adapter.sheet, draft),
    );
    this.chartPanel = new ChartPanel($("chart-panel"), {
      onEdit: (spec) => this.chartDialog.open(selectionRect(this.grid.selection), spec, (d) => this.saveChart(d)),
      onDelete: (spec) => this.deleteChart(spec),
      onSelectRange: (spec) => {
        this.grid.selectRect({ r1: spec.range.row1, c1: spec.range.col1, r2: spec.range.row2, c2: spec.range.col2 });
        this.grid.focus();
      },
      onClose: () => this.grid.focus(),
    });
    this.formulaBar = new FormulaBar(
      { nameBox: $("namebox"), formulaInput: $("formula"), valueOut: $("value") },
      this.grid,
      this.adapter,
      (text) => this.runFormulaBarCommand(text),
    );
    this.tabs = new SheetTabs($("sheet-tabs"), {
      onSelect: (i) => this.switchSheet(i),
      onRename: (i, name) => this.renameSheet(i, name),
      onDelete: (i) => this.deleteSheet(i),
      onAdd: () => this.openNewSheetDialog(),
    });

    this.autosaver = new AutoSaver({
      persist: (record) => {
        if (!this.currentId) return;
        saveWorkbook(
          storage,
          { id: this.currentId, name: this.currentName, sheets: this.wb.sheetCount, savedAt: record.savedAt },
          { activeSheet: record.activeSheet, workbook: record.workbook },
        );
      },
      snapshot: () => ({ workbook: this.wb.toJson(), activeSheet: this.adapter.sheet }),
      onState: (state, detail) => this.renderAutosaveState(state, detail),
    });
    this.home = new HomeScreen({
      onNew: () => this.createWorkbook(),
      onOpen: () => void this.openFile(),
      onImport: () => void this.openFile("csv"),
      onOpenRecent: (id) => this.openFromLibrary(id),
      onDeleteRecent: (id) => this.deleteFromLibrary(id),
      onTheme: (anchor) => this.openThemeMenu(anchor),
    });
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "hidden" && this.autosaver.pending) this.autosaver.flush();
    });
    window.addEventListener("beforeunload", () => {
      if (this.autosaver.pending) this.autosaver.flush();
    });

    $("grid")
      .querySelector(".ssgrid-viewport")
      ?.addEventListener("scroll", () => requestAnimationFrame(() => this.updateSelectionInfo()), { passive: true });
    this.decorateIcons();
    this.bindMenubar();
    this.bindToolbar();
    this.bindDialogs();
    this.renderShortcutHelp();
    $("boot").textContent = `spreadsheet-core ${bridgeVersion()} · wasm`;

    // Reopen the workbook that was open last; otherwise show the home screen.
    const index = readIndex(storage);
    if (index.current && this.openFromLibrary(index.current)) {
      /* opened */
    } else {
      this.showHome();
    }

    // Handy for debugging / e2e tests in the console.
    (window as unknown as { spreadsheet: unknown }).spreadsheet = {
      wb: () => this.wb,
      grid: this.grid,
      adapter: () => this.adapter,
      app: this,
    };

  }

  // =================================================================== state

  get workbook(): SpreadsheetWorkbook {
    return this.wb;
  }

  get activeSheet(): number {
    return this.adapter.sheet;
  }

  /** Every mutation of the workbook funnels through here. */
  private changed(): void {
    this.formulaBar.refresh();
    this.updateSelectionInfo();
    this.updateHistoryButtons();
    this.updateFormatToolbar();
    this.renderCharts();
    this.autosaver.markDirty();
  }

  private updateFormatToolbar(): void {
    this.formatToolbar.update(this.wb.getFormat(this.adapter.sheet, cellRef(this.grid.activeCell)));
  }

  private setStatus(message: string, kind: StatusKind): void {
    const node = $("status");
    node.textContent = message;
    node.className = `status-${kind}`;
  }

  private report(result: EditResult, what: string): void {
    if (result.ok) {
      const sleep = result.sleepSeconds ? ` (SLEEP ${result.sleepSeconds}s requested, not performed)` : "";
      this.setStatus(`${what}: ok${sleep}`, "ok");
    } else {
      this.setStatus(`${what}: ${result.status}`, "error");
    }
  }

  private updateSelectionInfo(): void {
    const sel = this.grid.selection;
    const rect = selectionRect(sel);
    const range = isSingleCell(sel)
      ? cellRef(sel.active)
      : `${cellRef({ row: rect.r1, col: rect.c1 })}:${cellRef({ row: rect.r2, col: rect.c2 })} (${rectCellCount(rect)} cells)`;
    $("sel-info").textContent = `"${this.adapter.name}" · ${this.adapter.rows} × ${this.adapter.cols} · ${range}`;
    const s = this.grid.stats;
    $("render-info").textContent =
      s.rect.r2 >= s.rect.r1
        ? `DOM cells: ${s.renderedCells} (rows ${s.rect.r1 + 1}–${s.rect.r2 + 1}, cols ${cellRef({ row: 0, col: s.rect.c1 }).replace(/\d+$/, "")}–${cellRef({ row: 0, col: s.rect.c2 }).replace(/\d+$/, "")})`
        : "";
  }

  private updateHistoryButtons(): void {
    $<HTMLButtonElement>("btn-undo").disabled = !this.wb.canUndo;
    $<HTMLButtonElement>("btn-redo").disabled = !this.wb.canRedo;
  }

  private renderAutosaveState(state: AutosaveState, detail?: string): void {
    const node = $("autosave");
    const time = () =>
      this.autosaver?.lastSavedAt ? new Date(this.autosaver.lastSavedAt).toLocaleTimeString() : "";
    const text: Record<AutosaveState, string> = {
      idle: "not saved yet",
      dirty: "unsaved changes…",
      saving: "saving…",
      saved: `saved ${time()}`,
      error: `save failed${detail ? `: ${detail}` : ""}`,
      disabled: "auto-save off",
    };
    node.textContent = text[state];
    node.dataset.state = state;
    node.title = "auto-saved on this device (browser storage); use File › Save .290 to keep a file";
  }

  // ================================================================== sheets

  private refreshSheetViews(): void {
    this.tabs.update(this.wb.sheetNames(), this.adapter.sheet);
    this.updateSelectionInfo();
    this.updateHistoryButtons();
    this.updateFormatToolbar();
    this.renderCharts();
  }

  // ============================================================== formatting

  /** Applies a format patch to the selection (Rust stores it; one undo step). */
  applyFormat(patch: FormatPatch): void {
    const rect = selectionRect(this.grid.selection);
    const result = this.wb.setFormat(this.adapter.sheet, { row1: rect.r1, col1: rect.c1, row2: rect.r2, col2: rect.c2 }, patch);
    this.grid.forceRender();
    this.changed();
    this.report(result, `format ${rectCellCount(rect)} cell(s)`);
    this.grid.focus();
  }

  // ================================================================= charts

  /** Opens the chart dialog for the current selection. */
  insertChart(): void {
    const rect = selectionRect(this.grid.selection);
    this.chartDialog.open(rect, null, (draft) => this.saveChart(draft));
  }

  private saveChart(draft: ChartDraft): void {
    if (draft.id !== undefined) {
      this.wb.updateChart(this.adapter.sheet, draft as ChartSpec);
      this.setStatus(`updated chart "${draft.title || draft.kind}"`, "ok");
    } else {
      const id = this.wb.addChart(this.adapter.sheet, draft);
      this.setStatus(`inserted chart #${id} (${draft.kind})`, "ok");
    }
    this.chartPanel.open();
    $("btn-charts").setAttribute("aria-pressed", "true");
    this.renderCharts();
    this.autosaver.markDirty();
    this.grid.focus();
  }

  private deleteChart(spec: ChartSpec): void {
    if (!window.confirm(`Delete chart "${spec.title || spec.kind}"?`)) return;
    this.wb.removeChart(this.adapter.sheet, spec.id);
    this.renderCharts();
    this.autosaver.markDirty();
    this.setStatus("chart deleted", "ok");
  }

  private renderCharts(): void {
    const charts = this.wb.charts(this.adapter.sheet);
    const badge = $("charts-count");
    badge.textContent = String(charts.length);
    badge.hidden = charts.length === 0;
    if (!this.chartPanel.isOpen) return;
    this.chartPanel.render(charts, (spec) => this.wb.chartData(this.adapter.sheet, spec));
  }

  toggleChartPanel(): void {
    if (this.chartPanel.isOpen) this.chartPanel.close();
    else this.chartPanel.open();
    $("btn-charts").setAttribute("aria-pressed", String(this.chartPanel.isOpen));
    this.renderCharts();
  }

  // =========================================================== context menu

  private openContextMenu(x: number, y: number): void {
    const sel = this.grid.selection;
    const rect = selectionRect(sel);
    const mod = this.isMac ? "⌘" : "Ctrl+";
    const multi = !isSingleCell(sel);
    const items: MenuItem[] = [
      { label: "Cut", icon: "cut", shortcut: `${mod}X`, action: () => void this.runCommand("cut") },
      { label: "Copy", icon: "copy", shortcut: `${mod}C`, action: () => void this.runCommand("copy") },
      { label: "Paste", icon: "paste", shortcut: `${mod}V`, action: () => void this.runCommand("paste") },
      { label: multi ? `Clear ${rectCellCount(rect)} cells` : "Clear contents", shortcut: "Del", action: () => this.grid.clearSelection() },
      { separator: true, label: "" },
      { label: "Insert chart…", icon: "chart", action: () => this.insertChart() },
      { label: "Sort range…", icon: "sort", disabled: !multi, action: () => this.openSortDialog() },
      { label: "Find / Replace…", icon: "find", action: () => this.openDialog("dlg-find", "find-text") },
      { separator: true, label: "" },
      { label: "Bold", icon: "bold", shortcut: `${mod}B`, action: () => this.applyFormat({ bold: !this.wb.getFormat(this.adapter.sheet, cellRef(sel.active)).bold }) },
      { label: "Clear formatting", icon: "clear-format", action: () => this.applyFormat({ clearAll: true }) },
    ];
    this.contextMenu.openAt(x, y, items);
  }

  switchSheet(index: number): void {
    if (index < 0 || index >= this.wb.sheetCount) return;
    if (this.grid.isEditing) this.grid.commitEdit("none");
    this.adapter = new SheetAdapter(this.wb, index);
    this.grid.setSource(this.adapter);
    this.formulaBar.setSource(this.adapter);
    this.findMatches = [];
    this.findCursor = -1;
    this.refreshSheetViews();
    this.autosaver.markDirty(); // remembers the active sheet
    this.grid.focus();
  }

  addSheet(name: string, rows: number, cols: number): boolean {
    try {
      const index = this.wb.addSheet(name.trim() || `Sheet ${this.wb.sheetCount + 1}`, rows, cols);
      this.switchSheet(index);
      this.setStatus(`created "${this.adapter.name}" (${rows} × ${cols})`, "ok");
      return true;
    } catch (err) {
      this.setStatus((err as Error).message, "error");
      return false;
    }
  }

  renameSheet(index: number, name: string): void {
    this.wb.renameSheet(index, name);
    this.refreshSheetViews();
    this.setStatus(`renamed sheet to "${name}"`, "ok");
    this.autosaver.markDirty();
    this.grid.focus();
  }

  deleteSheet(index: number): void {
    const name = this.wb.sheetName(index);
    if (this.wb.sheetCount === 1) {
      this.setStatus("cannot delete the only sheet", "error");
      return;
    }
    if (!window.confirm(`Delete sheet "${name}"? This cannot be undone.`)) return;
    this.wb.removeSheet(index); // clears the undo history, like the desktop
    this.clipboard.clear();
    this.grid.setMarker(null);
    const next = Math.min(index, this.wb.sheetCount - 1);
    this.switchSheet(next);
    this.setStatus(`deleted sheet "${name}"`, "ok");
  }

  // ================================================================ commands

  async runCommand(cmd: GridCommand): Promise<void> {
    switch (cmd) {
      case "copy":
      case "cut": {
        const rect = selectionRect(this.grid.selection);
        const n = await this.clipboard.copy(this.adapter, rect, cmd);
        this.grid.setMarker(rect);
        this.setStatus(`${cmd === "cut" ? "Cut" : "Copied"} ${n} cell(s) — select a target and paste`, "info");
        break;
      }
      case "paste": {
        const target = this.grid.activeCell;
        const out = await this.clipboard.paste(this.adapter, target);
        if (!out) {
          this.setStatus("Nothing to paste", "info");
          return;
        }
        this.grid.setMarker(this.clipboard.current ? this.clipboard.current.rect : null);
        this.grid.forceRender();
        this.grid.selectRect(out.rect, false);
        this.changed();
        const what = { move: "moved cells (references follow)", values: "pasted values", text: "pasted text" }[out.kind];
        this.report(out.result, what);
        break;
      }
      case "undo":
        this.applyHistory(this.wb.undo(), "undo");
        break;
      case "redo":
        this.applyHistory(this.wb.redo(), "redo");
        break;
      case "goto":
        this.formulaBar.focusNameBox();
        break;
    }
  }

  private applyHistory(out: HistoryOutcome | null, what: "undo" | "redo"): void {
    if (!out) {
      this.setStatus(`Nothing to ${what}`, "info");
      return;
    }
    if (out.sheet !== this.adapter.sheet) this.switchSheet(out.sheet);
    this.grid.forceRender();
    const first = out.references[0] ? parseRef(out.references[0]) : null;
    if (first) this.grid.setSelection(first);
    this.changed();
    this.report(out.result, `${what} "${out.label}"`);
  }

  /** Formula-bar text that is a command rather than a cell value (desktop `SORT(...)`). */
  private runFormulaBarCommand(text: string): boolean {
    const result = this.adapter.sortCommand(text);
    if (result === null) return false;
    this.grid.forceRender();
    this.changed();
    this.report(result, text.trim());
    return true;
  }

  // ============================================================ find / sort

  findNext(needle: string): void {
    const matches = this.adapter.find(needle, false).map((c) => c.reference);
    if (matches.join() !== this.findMatches.join()) {
      this.findMatches = matches;
      this.findCursor = -1;
    }
    if (matches.length === 0) {
      this.setStatus(`"${needle}" not found`, "info");
      return;
    }
    this.findCursor = (this.findCursor + 1) % matches.length;
    const pos = parseRef(matches[this.findCursor]);
    if (pos) this.grid.setSelection(pos);
    this.setStatus(`match ${this.findCursor + 1} of ${matches.length}: ${matches[this.findCursor]}`, "info");
  }

  replaceAll(find: string, replace: string): void {
    const out = this.adapter.findReplace(find, replace);
    this.grid.forceRender();
    this.changed();
    if (out.matched === 0) this.setStatus("No matches found", "info");
    else if (out.replaced === out.matched) this.setStatus(`Replaced ${out.replaced} instance(s)`, "ok");
    else this.setStatus(`Replaced ${out.replaced} of ${out.matched} matching cell(s) (the rest were rejected by the engine)`, "error");
    this.findMatches = [];
  }

  sort(rect: CellRect, key: string, ascending: boolean): void {
    const result = this.adapter.sortRange(rect, key, ascending);
    this.grid.forceRender();
    this.grid.selectRect(rect, false);
    this.changed();
    this.report(result, `sort ${cellRef({ row: rect.r1, col: rect.c1 })}:${cellRef({ row: rect.r2, col: rect.c2 })} by ${key} ${ascending ? "asc" : "desc"}`);
  }

  // =================================================================== files

  private engineLabel(): string {
    return `spreadsheet-core ${bridgeVersion()} · WebAssembly · ${this.isMac ? "⌘" : "Ctrl"} shortcuts`;
  }

  /** Shows the home screen (the current workbook stays saved in the library). */
  showHome(): void {
    if (this.grid.isEditing) this.grid.commitEdit("none");
    if (this.currentId && this.autosaver.pending) this.autosaver.flush();
    this.popup.close();
    this.contextMenu.close();
    $("app").hidden = true;
    this.home.show(readIndex(this.storage).items, this.engineLabel());
    setCurrent(this.storage, null);
  }

  private showApp(): void {
    this.home.hide();
    $("app").hidden = false;
    this.grid.forceRender();
    this.grid.focus();
    // The grid was laid out while hidden; refresh the render stats label.
    requestAnimationFrame(() => this.updateSelectionInfo());
  }

  /** New empty workbook (added to the library on first auto-save). */
  createWorkbook(name = "Untitled"): void {
    this.installWorkbook(SpreadsheetWorkbook.create(DEFAULT_ROWS, DEFAULT_COLS), 0, newId(), name);
    this.autosaver.markDirty();
    this.setStatus("new workbook", "ok");
  }

  /** Opens a library entry; returns false when it is missing / corrupt. */
  openFromLibrary(id: string): boolean {
    const stored = loadWorkbook(this.storage, id);
    const meta = readIndex(this.storage).items.find((i) => i.id === id);
    if (!stored || !meta) return false;
    try {
      const wb = SpreadsheetWorkbook.fromJson(stored.workbook);
      if (wb.sheetCount === 0) {
        wb.free();
        return false;
      }
      this.installWorkbook(wb, Math.min(stored.activeSheet, wb.sheetCount - 1), id, meta.name);
      this.autosaver.lastSavedAt = meta.savedAt;
      this.renderAutosaveState("saved");
      setCurrent(this.storage, id);
      this.setStatus(`opened "${meta.name}" (${wb.sheetCount} sheet(s)) from this device`, "info");
      return true;
    } catch (err) {
      console.warn("workbook could not be restored", err);
      return false;
    }
  }

  deleteFromLibrary(id: string): void {
    const meta = readIndex(this.storage).items.find((i) => i.id === id);
    if (!meta || !window.confirm(`Remove "${meta.name}" from this device?`)) return;
    deleteWorkbook(this.storage, id);
    this.home.renderRecent(readIndex(this.storage).items);
  }

  /** Closes the open workbook and returns to the home screen. */
  closeWorkbook(): void {
    this.showHome();
  }

  renameWorkbook(name: string): void {
    const trimmed = name.trim() || "Untitled";
    this.currentName = trimmed;
    $<HTMLInputElement>("wb-name").value = trimmed;
    if (this.currentId) renameWorkbook(this.storage, this.currentId, trimmed);
    this.autosaver.markDirty();
  }

  async openFile(kind: "any" | "csv" = "any"): Promise<void> {
    const file = await pickFile(kind === "csv" ? ".csv" : ".290,.json,.csv");
    if (!file) return;
    await this.importFile(file);
  }

  /**
   * Imports a `.290` workbook (as a new library entry) or a CSV: as a new
   * sheet of the open workbook, or as a new workbook from the home screen.
   */
  async importFile(file: File): Promise<void> {
    const kind = classifyFileName(file.name);
    if (!kind) {
      this.setStatus(`unsupported file "${file.name}" (use .290 or .csv)`, "error");
      return;
    }
    let text: string;
    try {
      text = await readFileText(file);
    } catch (err) {
      this.setStatus(`could not read "${file.name}": ${(err as Error).message}`, "error");
      return;
    }
    const baseName = file.name.replace(/\.(290|json|csv)$/i, "");
    if (kind === "290") {
      try {
        const wb = SpreadsheetWorkbook.fromJson(text);
        if (wb.sheetCount === 0) {
          wb.free();
          this.setStatus(`"${file.name}" contains no sheets`, "error");
          return;
        }
        this.installWorkbook(wb, 0, newId(), baseName || "Imported workbook");
        this.autosaver.markDirty();
        this.setStatus(`opened "${file.name}" (${wb.sheetCount} sheet(s))`, "ok");
      } catch (err) {
        this.setStatus(`"${file.name}" is not a valid workbook: ${(err as Error).message}`, "error");
      }
    } else if (this.home.isVisible) {
      const wb = SpreadsheetWorkbook.empty();
      wb.addSheetFromCsv(file.name, text);
      this.installWorkbook(wb, 0, newId(), baseName || "Imported CSV");
      this.autosaver.markDirty();
      this.setStatus(`CSV loaded successfully as "${file.name}"`, "ok");
    } else {
      const index = this.wb.addSheetFromCsv(file.name, text);
      this.switchSheet(index);
      this.setStatus(`CSV loaded successfully as sheet "${file.name}"`, "ok");
    }
  }

  /** Makes `wb` the open workbook (frees the previous one) and shows the app. */
  private installWorkbook(wb: SpreadsheetWorkbook, activeSheet: number, id: string, name: string): void {
    if (this.grid.isEditing) this.grid.cancelEdit();
    if (this.currentId && this.autosaver.pending) this.autosaver.flush();
    const old = this.wb;
    this.wb = wb;
    this.currentId = id;
    this.currentName = name;
    $<HTMLInputElement>("wb-name").value = name;
    this.clipboard.clear();
    this.grid.setMarker(null);
    this.adapter = new SheetAdapter(wb, activeSheet);
    this.grid.setSource(this.adapter);
    this.formulaBar.setSource(this.adapter);
    this.autosaver.cancel();
    this.autosaver.lastSavedAt = null;
    this.renderAutosaveState("idle");
    this.refreshSheetViews();
    this.formulaBar.refresh();
    old.free();
    this.showApp();
  }

  /** Opens the save dialog (downloads the desktop `.290` format). */
  openSaveDialog(): void {
    $<HTMLInputElement>("save-name").value = safeFileName(this.currentName);
    this.openDialog("dlg-save", "save-name");
  }

  downloadWorkbook(fileName: string): void {
    const name = withExtension(safeFileName(fileName), "290");
    downloadText(name, this.wb.toJsonPretty(), "application/json");
    this.setStatus(`downloaded ${name}`, "ok");
  }

  /** Downloads the current sheet's values as CSV (formulas become values). */
  downloadCsv(): void {
    const name = withExtension(safeFileName(this.adapter.name), "csv");
    downloadText(name, this.wb.toCsv(this.adapter.sheet), "text/csv");
    this.setStatus(`downloaded ${name}`, "ok");
  }

  // ================================================================== theme

  setTheme(id: string): void {
    applyTheme(id, this.storage);
    this.chartPanel.invalidate();
    this.renderCharts();
    this.chartDialog.refreshPreview();
    this.setStatus(`theme: ${THEMES.find((t) => t.id === id)?.name ?? id}`, "info");
  }

  private themeItems(): MenuItem[] {
    const active = currentTheme();
    return THEMES.map((t) => ({
      id: `theme-${t.id}`,
      label: t.name,
      checked: t.id === active,
      action: () => this.setTheme(t.id),
    }));
  }

  private openThemeMenu(anchor: HTMLElement): void {
    this.popup.open(this.themeItems(), anchor);
  }

  // ================================================================ toolbar

  /** Fills every `data-icon` placeholder with the SVG icon. */
  private decorateIcons(): void {
    for (const el of document.querySelectorAll<HTMLElement>("[data-icon]")) {
      const name = el.dataset.icon as IconName;
      const svg = icon(name, el.classList.contains("icon-slot") ? 15 : 16);
      if (el.classList.contains("btn-icon")) el.insertAdjacentHTML("afterbegin", svg);
      else el.innerHTML = svg;
    }
    $("brand-logo").innerHTML = icon("logo", 22);
    $("btn-theme").innerHTML = icon("theme");
  }

  private fileMenu(): MenuItem[] {
    return [
      { id: "btn-new", label: "New workbook", icon: "new", action: () => this.createWorkbook() },
      { id: "btn-open", label: "Open…", icon: "open", shortcut: ".290 / .csv", action: () => void this.openFile() },
      { id: "btn-import", label: "Import CSV as sheet…", icon: "csv", action: () => void this.openFile("csv") },
      { separator: true, label: "" },
      { id: "btn-save", label: "Save as .290…", icon: "save", action: () => this.openSaveDialog() },
      { id: "btn-csv", label: "Export sheet as CSV", icon: "download", action: () => this.downloadCsv() },
      { separator: true, label: "" },
      { id: "btn-close", label: "Close workbook (home)", icon: "home", action: () => this.closeWorkbook() },
    ];
  }

  private editMenu(): MenuItem[] {
    const mod = this.isMac ? "⌘" : "Ctrl+";
    const multi = !isSingleCell(this.grid.selection);
    return [
      { label: "Undo", icon: "undo", shortcut: `${mod}Z`, disabled: !this.wb.canUndo, action: () => void this.runCommand("undo") },
      { label: "Redo", icon: "redo", shortcut: `${mod}Shift+Z`, disabled: !this.wb.canRedo, action: () => void this.runCommand("redo") },
      { separator: true, label: "" },
      { label: "Cut", icon: "cut", shortcut: `${mod}X`, action: () => void this.runCommand("cut") },
      { label: "Copy", icon: "copy", shortcut: `${mod}C`, action: () => void this.runCommand("copy") },
      { label: "Paste", icon: "paste", shortcut: `${mod}V`, action: () => void this.runCommand("paste") },
      { label: "Clear contents", shortcut: "Del", action: () => this.grid.clearSelection() },
      { separator: true, label: "" },
      { label: "Find and replace…", icon: "find", shortcut: `${mod}F`, action: () => this.openDialog("dlg-find", "find-text") },
      { label: "Sort range…", icon: "sort", disabled: !multi, action: () => this.openSortDialog() },
      { label: "Go to cell…", shortcut: `${mod}G`, action: () => this.formulaBar.focusNameBox() },
    ];
  }

  private insertMenu(): MenuItem[] {
    return [
      { id: "btn-insert-sheet", label: "New sheet…", icon: "sheet", action: () => this.openNewSheetDialog() },
      { id: "btn-insert-chart", label: "Chart from selection…", icon: "chart", action: () => this.insertChart() },
      { separator: true, label: "" },
      { id: "btn-seed", label: "Sample data", action: () => this.seedSampleData() },
    ];
  }

  private formatMenu(): MenuItem[] {
    const mod = this.isMac ? "⌘" : "Ctrl+";
    const f = this.wb.getFormat(this.adapter.sheet, cellRef(this.grid.activeCell));
    return [
      { label: "Bold", icon: "bold", shortcut: `${mod}B`, checked: !!f.bold, action: () => this.applyFormat({ bold: !f.bold }) },
      { label: "Italic", icon: "italic", shortcut: `${mod}I`, checked: !!f.italic, action: () => this.applyFormat({ italic: !f.italic }) },
      { label: "Underline", icon: "underline", shortcut: `${mod}U`, checked: !!f.underline, action: () => this.applyFormat({ underline: !f.underline }) },
      { separator: true, label: "" },
      { label: "Align left", icon: "align-left", checked: f.align === "left", action: () => this.applyFormat({ align: "left" }) },
      { label: "Align center", icon: "align-center", checked: f.align === "center", action: () => this.applyFormat({ align: "center" }) },
      { label: "Align right", icon: "align-right", checked: f.align === "right", action: () => this.applyFormat({ align: "right" }) },
      { separator: true, label: "" },
      { label: "Clear formatting", icon: "clear-format", action: () => this.applyFormat({ clearAll: true }) },
    ];
  }

  private viewMenu(): MenuItem[] {
    return [
      { id: "btn-charts-menu", label: "Chart panel", icon: "panel-right", checked: this.chartPanel.isOpen, action: () => this.toggleChartPanel() },
      { separator: true, label: "" },
      ...this.themeItems(),
      { separator: true, label: "" },
      { id: "btn-shortcuts", label: "Keyboard shortcuts", icon: "keyboard", action: () => this.openDialog("dlg-shortcuts") },
    ];
  }

  private bindMenubar(): void {
    new MenuBar(
      $("menubar"),
      [
        { id: "file", label: "File", items: () => this.fileMenu() },
        { id: "edit", label: "Edit", items: () => this.editMenu() },
        { id: "insert", label: "Insert", items: () => this.insertMenu() },
        { id: "format", label: "Format", items: () => this.formatMenu() },
        { id: "view", label: "View", items: () => this.viewMenu() },
      ],
      this.popup,
    );
    $("btn-home").addEventListener("click", () => this.closeWorkbook());
    $("btn-theme").addEventListener("click", () => this.openThemeMenu($("btn-theme")));
    const nameInput = $<HTMLInputElement>("wb-name");
    nameInput.addEventListener("change", () => this.renameWorkbook(nameInput.value));
    nameInput.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter" || e.key === "Escape") {
        if (e.key === "Escape") nameInput.value = this.currentName;
        nameInput.blur();
        this.grid.focus();
      }
    });
  }

  private bindToolbar(): void {
    // Toolbar buttons hand the focus back to the grid so shortcuts keep working.
    const on = (id: string, fn: () => void) =>
      $(id).addEventListener("click", () => {
        fn();
        this.grid.focus();
      });
    on("btn-undo", () => void this.runCommand("undo"));
    on("btn-redo", () => void this.runCommand("redo"));
    on("btn-cut", () => void this.runCommand("cut"));
    on("btn-copy", () => void this.runCommand("copy"));
    on("btn-paste", () => void this.runCommand("paste"));
    on("btn-find", () => this.openDialog("dlg-find", "find-text"));
    on("btn-sort", () => this.openSortDialog());
    on("btn-chart", () => this.insertChart());
    on("btn-charts", () => this.toggleChartPanel());
    $("chart-panel-close").addEventListener("click", () => this.toggleChartPanel());
    // Formatting / find shortcuts (the grid does not know about them).
    $("grid").addEventListener("keydown", (e) => {
      const mod = this.isMac ? e.metaKey : e.ctrlKey;
      if (!mod || e.altKey || this.grid.isEditing) return;
      const key = e.key.toLowerCase();
      const active = this.wb.getFormat(this.adapter.sheet, cellRef(this.grid.activeCell));
      if (key === "b") this.applyFormat({ bold: !active.bold });
      else if (key === "i") this.applyFormat({ italic: !active.italic });
      else if (key === "u") this.applyFormat({ underline: !active.underline });
      else if (key === "f") this.openDialog("dlg-find", "find-text");
      else if (key === "s") this.openSaveDialog();
      else return;
      e.preventDefault();
    });
    // Drag & drop a .290 / .csv anywhere on the page.
    document.addEventListener("dragover", (e) => e.preventDefault());
    document.addEventListener("drop", (e) => {
      e.preventDefault();
      const file = e.dataTransfer?.files?.[0];
      if (file) void this.importFile(file);
    });
  }

  private openDialog(id: string, focusId?: string): void {
    const dlg = $<HTMLDialogElement>(id);
    if (!dlg.open) dlg.showModal();
    if (focusId) {
      const input = $<HTMLInputElement>(focusId);
      input.focus();
      input.select();
    }
  }

  openNewSheetDialog(): void {
    $<HTMLInputElement>("new-name").value = `Sheet ${this.wb.sheetCount + 1}`;
    this.openDialog("dlg-new-sheet", "new-name");
  }

  private openSortDialog(): void {
    const rect = selectionRect(this.grid.selection);
    $<HTMLInputElement>("sort-start").value = cellRef({ row: rect.r1, col: rect.c1 });
    $<HTMLInputElement>("sort-end").value = cellRef({ row: rect.r2, col: rect.c2 });
    $<HTMLInputElement>("sort-key").value = cellRef({ row: rect.r1, col: rect.c1 }).replace(/\d+$/, "");
    this.openDialog("dlg-sort", "sort-key");
  }

  private bindDialogs(): void {
    // Keep grid shortcuts out of dialogs.
    for (const dlg of document.querySelectorAll<HTMLDialogElement>("dialog")) {
      dlg.addEventListener("keydown", (e) => e.stopPropagation());
      dlg.addEventListener("close", () => this.grid.focus());
    }

    $("new-sheet-form").addEventListener("submit", (e) => {
      e.preventDefault();
      const rows = Number($<HTMLInputElement>("new-rows").value);
      const cols = Number($<HTMLInputElement>("new-cols").value);
      if (!Number.isInteger(rows) || !Number.isInteger(cols) || rows < 1 || cols < 1 || rows > MAX_ROWS || cols > MAX_COLS) {
        $("new-sheet-error").textContent = `rows must be 1–${MAX_ROWS} and columns 1–${MAX_COLS}`;
        return;
      }
      $("new-sheet-error").textContent = "";
      if (this.addSheet($<HTMLInputElement>("new-name").value, rows, cols)) $<HTMLDialogElement>("dlg-new-sheet").close();
    });

    $("save-form").addEventListener("submit", (e) => {
      e.preventDefault();
      const name = $<HTMLInputElement>("save-name").value.trim();
      if (!name) return;
      $<HTMLDialogElement>("dlg-save").close();
      this.downloadWorkbook(name);
    });

    $("find-next").addEventListener("click", () => {
      const needle = $<HTMLInputElement>("find-text").value;
      if (needle) this.findNext(needle);
    });
    $("find-text").addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        const needle = $<HTMLInputElement>("find-text").value;
        if (needle) this.findNext(needle);
      }
    });
    $("find-form").addEventListener("submit", (e) => {
      e.preventDefault();
      const find = $<HTMLInputElement>("find-text").value;
      if (!find) {
        this.setStatus("Please enter text to find", "error");
        return;
      }
      this.replaceAll(find, $<HTMLInputElement>("replace-text").value);
    });

    $("sort-form").addEventListener("submit", (e) => {
      e.preventDefault();
      const submitter = (e as SubmitEvent).submitter as HTMLButtonElement | null;
      const ascending = submitter?.value !== "desc";
      const start = parseRef($<HTMLInputElement>("sort-start").value);
      const end = parseRef($<HTMLInputElement>("sort-end").value);
      const key = $<HTMLInputElement>("sort-key").value.trim().toUpperCase();
      if (!start || !end || !key) {
        $("sort-error").textContent = "enter a range (e.g. A1 and C9) and a column letter or row number";
        return;
      }
      $("sort-error").textContent = "";
      this.sort({ r1: start.row, c1: start.col, r2: end.row, c2: end.col }, key, ascending);
      $<HTMLDialogElement>("dlg-sort").close();
    });
  }

  private renderShortcutHelp(): void {
    $("shortcuts").replaceChildren(
      ...shortcutHelp(this.isMac).map(([keys, what]) => {
        const li = document.createElement("li");
        const kbd = document.createElement("kbd");
        kbd.textContent = keys;
        li.append(kbd, ` ${what}`);
        return li;
      }),
    );
  }

  private seedSampleData(): void {
    const cmds: Array<[string, string]> = [
      ["A1", "10"], ["B1", "20"], ["C1", "A1+B1"], ["D1", "SUM(A1:C1)"], ["E1", "hello"], ["F1", "=D1/A1"],
      ["A2", "C1*2"], ["B2", "MAX(A1:D1)"], ["C2", "AVG(A1:B2)"], ["D2", "=A1/0"], ["E2", "=E1"],
    ];
    for (const [ref, text] of cmds) this.wb.editCell(this.adapter.sheet, ref, text);
    for (let r = 4; r < Math.min(this.adapter.rows, 300); r++) {
      this.wb.editCell(this.adapter.sheet, `A${r + 1}`, String(((r * 37) % 97) + 1));
    }
    this.wb.editCell(this.adapter.sheet, "B5", "SUM(A5:A300)");
    this.grid.forceRender();
    this.changed();
    this.setStatus("sample data written through the Rust core", "ok");
    this.grid.focus();
  }
}
