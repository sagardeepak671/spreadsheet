/**
 * Sheet tabs: switch (click), rename (double-click, inline), delete (×) and
 * create (+). Pure presentation; the host performs the workbook operations.
 */

import { icon } from "./icons";

export interface SheetTabsHandlers {
  onSelect(index: number): void;
  onRename(index: number, name: string): void;
  onDelete(index: number): void;
  onAdd(): void;
}

export class SheetTabs {
  private names: string[] = [];
  private active = 0;

  constructor(
    private readonly root: HTMLElement,
    private readonly handlers: SheetTabsHandlers,
  ) {
    root.classList.add("sheet-tabs");
  }

  update(names: string[], active: number): void {
    this.names = names;
    this.active = active;
    this.render();
  }

  /** Starts inline renaming of a tab (also used by the toolbar/menu). */
  beginRename(index: number): void {
    const tab = this.root.querySelector<HTMLElement>(`[data-index="${index}"]`);
    if (!tab) return;
    const label = tab.querySelector<HTMLElement>(".sheet-tab-name");
    if (!label) return;
    const input = document.createElement("input");
    input.className = "sheet-tab-rename";
    input.value = this.names[index];
    input.setAttribute("aria-label", "sheet name");
    label.replaceWith(input);
    input.focus();
    input.select();
    let done = false;
    const finish = (commit: boolean) => {
      if (done) return;
      done = true;
      const name = input.value.trim();
      if (commit && name && name !== this.names[index]) this.handlers.onRename(index, name);
      else this.render();
    };
    input.addEventListener("keydown", (e) => {
      e.stopPropagation();
      if (e.key === "Enter") finish(true);
      else if (e.key === "Escape") finish(false);
    });
    input.addEventListener("blur", () => finish(true));
  }

  private render(): void {
    const tabs = this.names.map((name, index) => {
      const tab = document.createElement("div");
      tab.className = "sheet-tab" + (index === this.active ? " is-active" : "");
      tab.dataset.index = String(index);
      tab.setAttribute("role", "tab");
      tab.setAttribute("aria-selected", String(index === this.active));

      const label = document.createElement("span");
      label.className = "sheet-tab-name";
      label.textContent = name;
      label.title = "double-click to rename";
      tab.appendChild(label);

      const close = document.createElement("button");
      close.className = "sheet-tab-close";
      close.type = "button";
      close.title = `delete ${name}`;
      close.setAttribute("aria-label", `delete sheet ${name}`);
      close.innerHTML = icon("close", 12);
      close.addEventListener("click", (e) => {
        e.stopPropagation();
        this.handlers.onDelete(index);
      });
      tab.appendChild(close);

      tab.addEventListener("click", () => {
        if (index !== this.active) this.handlers.onSelect(index);
      });
      tab.addEventListener("dblclick", () => this.beginRename(index));
      return tab;
    });

    const add = document.createElement("button");
    add.className = "sheet-tab-add btn-icon";
    add.type = "button";
    add.title = "new sheet";
    add.setAttribute("aria-label", "new sheet");
    add.innerHTML = icon("plus", 14);
    add.addEventListener("click", () => this.handlers.onAdd());

    this.root.replaceChildren(...tabs, add);
  }
}
