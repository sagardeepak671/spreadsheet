/**
 * Cell formatting helpers (presentation only). The format itself lives in
 * the Rust workbook; this turns it into CSS and computes toolbar toggles.
 */

import type { CellFormat, FormatPatch } from "./bridge";

export const FONT_SIZES = [10, 11, 12, 13, 14, 16, 18, 20, 24, 28, 32];
export const DEFAULT_FONT_SIZE = 13;

/** Inline CSS for a cell element (empty string for the default format). */
export function formatToCss(format: CellFormat | undefined): string {
  if (!format) return "";
  const parts: string[] = [];
  if (format.bold) parts.push("font-weight:700");
  if (format.italic) parts.push("font-style:italic");
  if (format.underline) parts.push("text-decoration:underline");
  if (format.fontSize) parts.push(`font-size:${format.fontSize}px`);
  const fill = format.fill && isHexColor(format.fill) ? format.fill : null;
  if (format.color && isHexColor(format.color)) parts.push(`color:${format.color}`);
  // A fill without an explicit text colour must stay readable in every theme:
  // dark text on light fills, light text on dark fills.
  else if (fill) parts.push(`color:${isLightColor(fill) ? "#1f2328" : "#f5f5f5"}`);
  if (fill) parts.push(`background-color:${fill}`);
  if (format.align) parts.push(`text-align:${format.align}`);
  return parts.join(";");
}

/** Relative luminance (sRGB) of a `#rrggbb` colour, 0..1. */
export function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

export function isLightColor(hex: string): boolean {
  return luminance(hex) > 0.4;
}

export function isHexColor(value: string): boolean {
  return /^#[0-9a-fA-F]{6}$/.test(value);
}

/** Toggle patch for a boolean property based on the active cell's state. */
export function togglePatch(active: CellFormat, key: "bold" | "italic" | "underline"): FormatPatch {
  return { [key]: !active[key] };
}

/** Normalises a `<input type=color>` value; `null` clears. */
export function colorPatch(key: "color" | "fill", value: string | null): FormatPatch {
  return { [key]: value && isHexColor(value) ? value.toLowerCase() : null };
}
