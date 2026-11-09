import { describe, expect, it } from "vitest";

import { AUTOSAVE_KEY, type KeyValueStorage } from "./autosave";
import {
  deleteWorkbook,
  formatBytes,
  formatWhen,
  LIBRARY_KEY,
  loadWorkbook,
  migrateLegacyAutosave,
  newId,
  readIndex,
  renameWorkbook,
  saveWorkbook,
  setCurrent,
} from "./library";

function fakeStorage(): KeyValueStorage & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v), removeItem: (k) => void data.delete(k) };
}

describe("workbook library", () => {
  it("saves, lists (most recent first), loads, renames and deletes", () => {
    const storage = fakeStorage();
    expect(readIndex(storage)).toEqual({ version: 1, current: null, items: [] });
    saveWorkbook(storage, { id: "a", name: "A", sheets: 1, savedAt: 100 }, { activeSheet: 0, workbook: "[1]" });
    saveWorkbook(storage, { id: "b", name: "B", sheets: 2, savedAt: 200 }, { activeSheet: 1, workbook: "[1,2]" });
    let index = readIndex(storage);
    expect(index.current).toBe("b");
    expect(index.items.map((i) => i.id)).toEqual(["b", "a"]);
    expect(index.items[0].bytes).toBeGreaterThan(10);

    saveWorkbook(storage, { id: "a", name: "A", sheets: 1, savedAt: 300 }, { activeSheet: 0, workbook: "[1]" });
    index = readIndex(storage);
    expect(index.items.map((i) => i.id)).toEqual(["a", "b"]); // re-saving moves it to the front
    expect(index.items).toHaveLength(2);

    expect(loadWorkbook(storage, "b")).toEqual({ version: 1, activeSheet: 1, workbook: "[1,2]" });
    expect(loadWorkbook(storage, "zzz")).toBeNull();

    renameWorkbook(storage, "b", "Budget");
    expect(readIndex(storage).items.find((i) => i.id === "b")?.name).toBe("Budget");
    setCurrent(storage, null);
    expect(readIndex(storage).current).toBeNull();

    deleteWorkbook(storage, "a");
    expect(readIndex(storage).items.map((i) => i.id)).toEqual(["b"]);
    expect(loadWorkbook(storage, "a")).toBeNull();
  });

  it("survives corrupt data and generates unique ids", () => {
    const storage = fakeStorage();
    storage.setItem(LIBRARY_KEY, "{oops");
    expect(readIndex(storage).items).toEqual([]);
    storage.setItem("spreadsheet.workbook.x", "{bad json");
    expect(loadWorkbook(storage, "x")).toBeNull();
    const ids = new Set(Array.from({ length: 50 }, () => newId()));
    expect(ids.size).toBe(50);
  });

  it("migrates the legacy single auto-save slot once", () => {
    const storage = fakeStorage();
    expect(migrateLegacyAutosave(storage, () => 1)).toBeNull();
    storage.setItem(AUTOSAVE_KEY, JSON.stringify({ version: 1, savedAt: 123, activeSheet: 1, workbook: "[1,2]" }));
    const meta = migrateLegacyAutosave(storage, (json) => JSON.parse(json).length);
    expect(meta).toMatchObject({ name: "Recovered workbook", sheets: 2, savedAt: 123 });
    expect(storage.getItem(AUTOSAVE_KEY)).toBeNull();
    expect(loadWorkbook(storage, meta!.id)?.activeSheet).toBe(1);
    expect(migrateLegacyAutosave(storage, () => 1)).toBeNull(); // nothing left to migrate
    // an unreadable legacy workbook is dropped instead of crashing
    storage.setItem(AUTOSAVE_KEY, JSON.stringify({ version: 1, workbook: "garbage" }));
    expect(
      migrateLegacyAutosave(storage, () => {
        throw new Error("bad");
      }),
    ).toBeNull();
    expect(storage.getItem(AUTOSAVE_KEY)).toBeNull();
  });

  it("formats sizes and relative times for the recent list", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(3 * 1024 * 1024)).toBe("3.0 MB");
    const now = 10_000_000;
    expect(formatWhen(now - 10_000, now)).toBe("just now");
    expect(formatWhen(now - 5 * 60_000, now)).toBe("5 min ago");
    expect(formatWhen(now - 3 * 3_600_000, now)).toBe("3 h ago");
    expect(formatWhen(now - 3 * 86_400_000, now)).toMatch(/\d/);
  });
});
