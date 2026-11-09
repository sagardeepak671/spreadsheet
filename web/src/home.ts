/**
 * Home screen: start actions + "recent on this device" list from the local
 * library. Pure presentation; the app performs the actions.
 */

import { icon } from "./icons";
import { formatBytes, formatWhen, type WorkbookMeta } from "./library";

export interface HomeHandlers {
  onNew(): void;
  onOpen(): void;
  onImport(): void;
  onOpenRecent(id: string): void;
  onDeleteRecent(id: string): void;
  onTheme(anchor: HTMLElement): void;
}

const $ = <T extends HTMLElement>(id: string): T => {
  const node = document.getElementById(id);
  if (!node) throw new Error(`missing #${id}`);
  return node as T;
};

export class HomeScreen {
  private readonly root = $("home");

  constructor(private readonly handlers: HomeHandlers) {
    $("home-logo").innerHTML = icon("logo", 40);
    for (const slot of this.root.querySelectorAll<HTMLElement>(".icon-slot[data-icon]")) {
      slot.innerHTML = icon(slot.dataset.icon as Parameters<typeof icon>[0], 22);
    }
    const themeBtn = $("home-theme");
    themeBtn.innerHTML = icon("theme");
    themeBtn.addEventListener("click", () => handlers.onTheme(themeBtn));
    $("home-new").addEventListener("click", () => handlers.onNew());
    $("home-open").addEventListener("click", () => handlers.onOpen());
    $("home-import").addEventListener("click", () => handlers.onImport());
  }

  get isVisible(): boolean {
    return !this.root.hidden;
  }

  show(recent: WorkbookMeta[], engine: string): void {
    $("home-engine").textContent = engine;
    this.renderRecent(recent);
    this.root.hidden = false;
    $("home-new").focus();
  }

  hide(): void {
    this.root.hidden = true;
  }

  renderRecent(recent: WorkbookMeta[]): void {
    const list = $("home-recent-list");
    if (recent.length === 0) {
      const empty = document.createElement("p");
      empty.className = "recent-empty";
      empty.textContent = "Nothing yet — workbooks you work on are auto-saved here.";
      list.replaceChildren(empty);
      return;
    }
    list.replaceChildren(
      ...recent.map((item) => {
        const row = document.createElement("div");
        row.className = "recent-row";
        row.dataset.id = item.id;
        const name = document.createElement("button");
        name.type = "button";
        name.className = "recent-name";
        name.textContent = item.name;
        name.title = "open";
        name.addEventListener("click", () => this.handlers.onOpenRecent(item.id));
        const meta = document.createElement("span");
        meta.className = "recent-meta";
        meta.textContent = `${item.sheets} sheet${item.sheets === 1 ? "" : "s"} · ${formatBytes(item.bytes)}`;
        const when = document.createElement("span");
        when.className = "recent-meta";
        when.textContent = formatWhen(item.savedAt);
        when.title = new Date(item.savedAt).toLocaleString();
        const del = document.createElement("button");
        del.type = "button";
        del.className = "btn-icon danger";
        del.title = "remove from this device";
        del.setAttribute("aria-label", `delete ${item.name}`);
        del.innerHTML = icon("trash");
        del.addEventListener("click", () => this.handlers.onDeleteRecent(item.id));
        row.append(name, meta, when, del);
        return row;
      }),
    );
  }
}
