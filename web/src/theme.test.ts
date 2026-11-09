import { describe, expect, it } from "vitest";

import { icon, ICON_NAMES } from "./icons";
import { DEFAULT_THEME, isThemeId, readTheme, THEME_KEY, THEMES } from "./theme";

describe("themes", () => {
  it("has four themes and validates ids", () => {
    expect(THEMES.map((t) => t.id)).toEqual(["light", "dark", "midnight", "warm"]);
    expect(THEMES.filter((t) => t.dark).map((t) => t.id)).toEqual(["dark", "midnight"]);
    expect(isThemeId("warm")).toBe(true);
    expect(isThemeId("neon")).toBe(false);
    expect(isThemeId(null)).toBe(false);
  });

  it("reads the stored theme with a safe fallback", () => {
    expect(readTheme({ getItem: () => null })).toBe(DEFAULT_THEME);
    expect(readTheme({ getItem: (k) => (k === THEME_KEY ? "midnight" : null) })).toBe("midnight");
    expect(readTheme({ getItem: () => "bogus" })).toBe(DEFAULT_THEME);
  });
});

describe("icons", () => {
  it("renders every icon as an inline SVG using currentColor", () => {
    expect(ICON_NAMES.length).toBeGreaterThan(30);
    for (const name of ICON_NAMES) {
      const svg = icon(name, 18);
      expect(svg.startsWith("<svg")).toBe(true);
      expect(svg).toContain('width="18"');
      expect(svg).toContain('stroke="currentColor"');
      expect(svg).toContain(`icon-${name}`);
      expect(svg).not.toMatch(/[\u{1F300}-\u{1FAFF}]/u); // no emoji
    }
  });
});
