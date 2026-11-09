import { describe, expect, it } from "vitest";

import { classifyFileName, safeFileName, withExtension } from "./files";

describe("file helpers", () => {
  it("classifies files like the desktop Open dialog", () => {
    expect(classifyFileName("book.290")).toBe("290");
    expect(classifyFileName("BOOK.290")).toBe("290");
    expect(classifyFileName("export.json")).toBe("290");
    expect(classifyFileName("data.csv")).toBe("csv");
    expect(classifyFileName("notes.txt")).toBeNull();
  });

  it("adds the extension once, like the desktop Save", () => {
    expect(withExtension("sheet", "290")).toBe("sheet.290");
    expect(withExtension("sheet.290", "290")).toBe("sheet.290");
    expect(withExtension("  ", "csv")).toBe("sheet.csv");
    expect(safeFileName("Q1/Q2: plan?")).toBe("Q1_Q2_ plan_");
    expect(safeFileName("   ")).toBe("sheet");
  });
});
