import { describe, expect, it } from "vitest";

import { colorPatch, formatToCss, isHexColor, isLightColor, luminance, togglePatch } from "./format";

describe("format helpers", () => {
  it("turns a CellFormat into inline CSS", () => {
    expect(formatToCss(undefined)).toBe("");
    expect(formatToCss({})).toBe("");
    expect(formatToCss({ bold: true, italic: true, underline: true })).toBe("font-weight:700;font-style:italic;text-decoration:underline");
    expect(formatToCss({ fontSize: 18, color: "#ff0000", fill: "#ffff00", align: "center" })).toBe(
      "font-size:18px;color:#ff0000;background-color:#ffff00;text-align:center",
    );
    // a fill without an explicit text colour gets a readable one (theme independent)
    expect(formatToCss({ fill: "#ffe082" })).toBe("color:#1f2328;background-color:#ffe082");
    expect(formatToCss({ fill: "#1f2a48" })).toBe("color:#f5f5f5;background-color:#1f2a48");
    expect(isLightColor("#ffffff")).toBe(true);
    expect(isLightColor("#000000")).toBe(false);
    expect(luminance("#808080")).toBeCloseTo(0.216, 2);
    // never injects unvalidated colour strings into CSS
    expect(formatToCss({ color: "red; background:url(x)" })).toBe("");
  });

  it("builds toggle and colour patches", () => {
    expect(togglePatch({ bold: true }, "bold")).toEqual({ bold: false });
    expect(togglePatch({}, "italic")).toEqual({ italic: true });
    expect(colorPatch("fill", "#ABCDEF")).toEqual({ fill: "#abcdef" });
    expect(colorPatch("color", null)).toEqual({ color: null });
    expect(colorPatch("color", "nope")).toEqual({ color: null });
    expect(isHexColor("#123abc")).toBe(true);
    expect(isHexColor("#123")).toBe(false);
  });
});
