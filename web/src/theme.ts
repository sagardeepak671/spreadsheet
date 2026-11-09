/**
 * Themes are sets of CSS custom properties applied to `<html data-theme>`;
 * the grid, toolbar, menus, dialogs and selection all read those variables.
 * Charts are SVG and cannot use CSS variables in their string renderer, so
 * `chartTheme()` reads the current values and hands them to the renderer.
 */

export interface ThemeInfo {
  id: string;
  name: string;
  description: string;
  dark: boolean;
}

export const THEMES: ThemeInfo[] = [
  { id: "light", name: "Clean light", description: "neutral greys, blue accent", dark: false },
  { id: "dark", name: "Dark", description: "soft dark surfaces", dark: true },
  { id: "midnight", name: "Midnight", description: "deep navy with cyan accents", dark: true },
  { id: "warm", name: "Warm", description: "paper tones with an amber accent", dark: false },
];

export const THEME_KEY = "spreadsheet.theme.v1";
export const DEFAULT_THEME = "light";

export function isThemeId(id: string | null | undefined): id is string {
  return !!id && THEMES.some((t) => t.id === id);
}

export function readTheme(storage: Pick<Storage, "getItem">): string {
  const id = storage.getItem(THEME_KEY);
  return isThemeId(id) ? id : DEFAULT_THEME;
}

export function applyTheme(id: string, storage?: Pick<Storage, "setItem">): void {
  const theme = isThemeId(id) ? id : DEFAULT_THEME;
  document.documentElement.dataset.theme = theme;
  const meta = document.querySelector<HTMLMetaElement>('meta[name="color-scheme"]');
  if (meta) meta.content = THEMES.find((t) => t.id === theme)?.dark ? "dark" : "light";
  storage?.setItem(THEME_KEY, theme);
}

export function currentTheme(): string {
  return document.documentElement.dataset.theme ?? DEFAULT_THEME;
}

/** Colours a chart renderer needs, resolved from the active theme. */
export interface ChartTheme {
  background: string;
  text: string;
  muted: string;
  grid: string;
  axis: string;
  series: string[];
}

export function chartTheme(): ChartTheme {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  return {
    background: v("--chart-bg", "#ffffff"),
    text: v("--chart-text", "#1f2328"),
    muted: v("--chart-muted", "#6b7280"),
    grid: v("--chart-grid", "#e5e7eb"),
    axis: v("--chart-axis", "#9aa0a6"),
    series: [1, 2, 3, 4, 5, 6, 7, 8].map((i) => v(`--chart-${i}`, "")).filter(Boolean),
  };
}
