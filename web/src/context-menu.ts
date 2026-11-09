/**
 * Popup menu used for the menubar dropdowns and the right-click menu.
 * Items are plain callbacks; the host decides what is relevant.
 */

import { icon, type IconName } from "./icons";

export interface MenuItem {
  label: string;
  /** Stable id for tests / lookup (`btn-new`). */
  id?: string;
  icon?: IconName;
  shortcut?: string;
  disabled?: boolean;
  checked?: boolean;
  action?: () => void;
  /** Renders a separator instead of an item. */
  separator?: boolean;
}

export class PopupMenu {
  private readonly root: HTMLDivElement;
  private onClose: (() => void) | null = null;

  constructor() {
    this.root = document.createElement("div");
    this.root.className = "popup-menu";
    this.root.setAttribute("role", "menu");
    this.root.style.display = "none";
    document.body.appendChild(this.root);
    document.addEventListener("pointerdown", (e) => {
      if (this.isOpen && !this.root.contains(e.target as Node)) this.close();
    });
    document.addEventListener("keydown", (e) => {
      if (!this.isOpen) return;
      if (e.key === "Escape") {
        this.close();
        return;
      }
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const buttons = [...this.root.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
        const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
        const next = e.key === "ArrowDown" ? (i + 1) % buttons.length : (i - 1 + buttons.length) % buttons.length;
        buttons[next]?.focus();
      }
    });
    window.addEventListener("blur", () => this.close());
    window.addEventListener("resize", () => this.close());
  }

  get isOpen(): boolean {
    return this.root.style.display !== "none";
  }

  get element(): HTMLElement {
    return this.root;
  }

  /** Opens at a point (context menu) or below an anchor element (menubar). */
  open(items: MenuItem[], at: { x: number; y: number } | HTMLElement, onClose?: () => void): void {
    this.onClose?.();
    this.onClose = onClose ?? null;
    this.root.replaceChildren(...items.map((item) => this.renderItem(item)));
    this.root.style.display = "block";
    const rect = this.root.getBoundingClientRect();
    let x: number;
    let y: number;
    if (at instanceof HTMLElement) {
      const a = at.getBoundingClientRect();
      x = a.left;
      y = a.bottom + 2;
    } else {
      x = at.x;
      y = at.y;
    }
    const { innerWidth, innerHeight } = window;
    this.root.style.left = `${Math.max(4, Math.min(x, innerWidth - rect.width - 4))}px`;
    this.root.style.top = `${Math.max(4, Math.min(y, innerHeight - rect.height - 4))}px`;
    (this.root.querySelector("button:not(:disabled)") as HTMLButtonElement | null)?.focus();
  }

  close(): void {
    if (!this.isOpen) return;
    this.root.style.display = "none";
    const cb = this.onClose;
    this.onClose = null;
    cb?.();
  }

  private renderItem(item: MenuItem): HTMLElement {
    if (item.separator) {
      const hr = document.createElement("div");
      hr.className = "popup-menu-sep";
      return hr;
    }
    const button = document.createElement("button");
    button.type = "button";
    button.className = "popup-menu-item";
    button.setAttribute("role", item.checked === undefined ? "menuitem" : "menuitemcheckbox");
    if (item.checked !== undefined) button.setAttribute("aria-checked", String(item.checked));
    if (item.id) button.id = item.id;
    button.disabled = !!item.disabled;
    const mark = document.createElement("span");
    mark.className = "popup-menu-icon";
    if (item.checked) mark.innerHTML = icon("check", 14);
    else if (item.icon) mark.innerHTML = icon(item.icon, 16);
    const label = document.createElement("span");
    label.className = "popup-menu-label";
    label.textContent = item.label;
    button.append(mark, label);
    if (item.shortcut) {
      const kbd = document.createElement("kbd");
      kbd.textContent = item.shortcut;
      button.appendChild(kbd);
    }
    button.addEventListener("click", () => {
      this.close();
      item.action?.();
    });
    return button;
  }
}

/** Backwards-compatible name used by the grid's right-click handler. */
export class ContextMenu extends PopupMenu {
  constructor() {
    super();
    this.element.classList.add("context-menu");
  }

  openAt(x: number, y: number, items: MenuItem[]): void {
    this.open(items, { x, y });
  }
}

export interface MenuDefinition {
  id: string;
  label: string;
  items: () => MenuItem[];
}

/** Menubar: one button per menu, hover switches between open menus. */
export class MenuBar {
  private openId: string | null = null;
  private readonly buttons = new Map<string, HTMLButtonElement>();

  constructor(
    root: HTMLElement,
    private readonly menus: MenuDefinition[],
    private readonly popup: PopupMenu,
  ) {
    root.setAttribute("role", "menubar");
    for (const menu of menus) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "menubar-item";
      button.id = `menu-${menu.id}`;
      button.textContent = menu.label;
      button.setAttribute("role", "menuitem");
      button.setAttribute("aria-haspopup", "menu");
      button.setAttribute("aria-expanded", "false");
      button.addEventListener("pointerdown", (e) => {
        e.preventDefault();
        e.stopPropagation(); // the document listener would close the menu we just opened
        if (this.openId === menu.id) this.popup.close();
        else this.show(menu.id);
      });
      button.addEventListener("pointerenter", () => {
        if (this.openId && this.openId !== menu.id) this.show(menu.id);
      });
      button.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " " || e.key === "ArrowDown") {
          e.preventDefault();
          this.show(menu.id);
        }
      });
      root.appendChild(button);
      this.buttons.set(menu.id, button);
    }
  }

  show(id: string): void {
    const menu = this.menus.find((m) => m.id === id);
    const button = this.buttons.get(id);
    if (!menu || !button) return;
    this.buttons.forEach((b) => {
      b.classList.remove("is-open");
      b.setAttribute("aria-expanded", "false");
    });
    button.classList.add("is-open");
    button.setAttribute("aria-expanded", "true");
    this.openId = id;
    this.popup.open(menu.items(), button, () => {
      if (this.openId === id) {
        this.openId = null;
        button.classList.remove("is-open");
        button.setAttribute("aria-expanded", "false");
      }
    });
  }
}
