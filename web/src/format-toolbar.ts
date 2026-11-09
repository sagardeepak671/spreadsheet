/**
 * Formatting toolbar: bold / italic / underline, font size, text & fill
 * colour, horizontal alignment, clear formatting. The buttons mirror the
 * active cell's format (read from Rust) and apply patches to the selection.
 */

import type { CellFormat, FormatPatch } from "./bridge";
import { colorPatch, DEFAULT_FONT_SIZE, FONT_SIZES, togglePatch } from "./format";

export interface FormatToolbarElements {
  bold: HTMLButtonElement;
  italic: HTMLButtonElement;
  underline: HTMLButtonElement;
  fontSize: HTMLSelectElement;
  color: HTMLInputElement;
  colorClear: HTMLButtonElement;
  fill: HTMLInputElement;
  fillClear: HTMLButtonElement;
  alignLeft: HTMLButtonElement;
  alignCenter: HTMLButtonElement;
  alignRight: HTMLButtonElement;
  clear: HTMLButtonElement;
}

export class FormatToolbar {
  private active: CellFormat = {};

  constructor(
    private readonly els: FormatToolbarElements,
    apply: (patch: FormatPatch) => void,
  ) {
    els.fontSize.replaceChildren(
      ...FONT_SIZES.map((size) => {
        const opt = document.createElement("option");
        opt.value = String(size);
        opt.textContent = String(size);
        return opt;
      }),
    );
    els.bold.addEventListener("click", () => apply(togglePatch(this.active, "bold")));
    els.italic.addEventListener("click", () => apply(togglePatch(this.active, "italic")));
    els.underline.addEventListener("click", () => apply(togglePatch(this.active, "underline")));
    els.fontSize.addEventListener("change", () => {
      const size = Number(els.fontSize.value);
      apply({ fontSize: size === DEFAULT_FONT_SIZE ? null : size });
    });
    els.color.addEventListener("input", () => apply(colorPatch("color", els.color.value)));
    els.colorClear.addEventListener("click", () => apply(colorPatch("color", null)));
    els.fill.addEventListener("input", () => apply(colorPatch("fill", els.fill.value)));
    els.fillClear.addEventListener("click", () => apply(colorPatch("fill", null)));
    els.alignLeft.addEventListener("click", () => apply({ align: this.active.align === "left" ? null : "left" }));
    els.alignCenter.addEventListener("click", () => apply({ align: this.active.align === "center" ? null : "center" }));
    els.alignRight.addEventListener("click", () => apply({ align: this.active.align === "right" ? null : "right" }));
    els.clear.addEventListener("click", () => apply({ clearAll: true }));
  }

  /** Reflects the active cell's format in the toolbar. */
  update(format: CellFormat): void {
    this.active = format;
    const { els } = this;
    setPressed(els.bold, !!format.bold);
    setPressed(els.italic, !!format.italic);
    setPressed(els.underline, !!format.underline);
    els.fontSize.value = String(format.fontSize ?? DEFAULT_FONT_SIZE);
    const css = getComputedStyle(document.documentElement);
    const themeHex = (name: string, fallback: string) => {
      const v = css.getPropertyValue(name).trim();
      return /^#[0-9a-fA-F]{6}$/.test(v) ? v : fallback;
    };
    els.color.value = format.color ?? themeHex("--fg", "#000000");
    els.fill.value = format.fill ?? themeHex("--bg", "#ffffff");
    setPressed(els.alignLeft, format.align === "left");
    setPressed(els.alignCenter, format.align === "center");
    setPressed(els.alignRight, format.align === "right");
  }
}

function setPressed(button: HTMLButtonElement, pressed: boolean): void {
  button.setAttribute("aria-pressed", String(pressed));
  button.classList.toggle("is-pressed", pressed);
}
