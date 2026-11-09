import { describe, expect, it } from "vitest";

import { AUTOSAVE_KEY, AutoSaver, clearAutosave, readAutosave, writeAutosave, type AutosaveState, type KeyValueStorage } from "./autosave";

function fakeStorage(): KeyValueStorage & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return {
    data,
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => void data.set(k, v),
    removeItem: (k) => void data.delete(k),
  };
}

/** Manual timers so the debounce is deterministic. */
function fakeTimers() {
  const queue: Array<{ fn: () => void; at: number }> = [];
  let now = 1000;
  return {
    now: () => now,
    setTimer: (fn: () => void, ms: number) => {
      const entry = { fn, at: now + ms };
      queue.push(entry);
      return entry;
    },
    clearTimer: (h: unknown) => {
      const i = queue.indexOf(h as { fn: () => void; at: number });
      if (i >= 0) queue.splice(i, 1);
    },
    advance(ms: number) {
      now += ms;
      for (const entry of [...queue].sort((a, b) => a.at - b.at)) {
        if (entry.at <= now) {
          queue.splice(queue.indexOf(entry), 1);
          entry.fn();
        }
      }
    },
    pending: () => queue.length,
  };
}

describe("autosave record", () => {
  it("round-trips and ignores unknown/corrupt data", () => {
    const storage = fakeStorage();
    expect(readAutosave(storage)).toBeNull();
    writeAutosave(storage, { version: 1, savedAt: 123, activeSheet: 2, workbook: "[]" });
    expect(readAutosave(storage)).toEqual({ version: 1, savedAt: 123, activeSheet: 2, workbook: "[]" });
    storage.setItem(AUTOSAVE_KEY, "{not json");
    expect(readAutosave(storage)).toBeNull();
    storage.setItem(AUTOSAVE_KEY, JSON.stringify({ version: 2, workbook: "[]" }));
    expect(readAutosave(storage)).toBeNull();
    storage.setItem(AUTOSAVE_KEY, JSON.stringify({ version: 1, workbook: "[]" }));
    expect(readAutosave(storage)).toEqual({ version: 1, savedAt: 0, activeSheet: 0, workbook: "[]" });
    clearAutosave(storage);
    expect(readAutosave(storage)).toBeNull();
  });
});

describe("AutoSaver", () => {
  it("debounces writes and reports its state", () => {
    const storage = fakeStorage();
    const timers = fakeTimers();
    const states: AutosaveState[] = [];
    let version = 0;
    const saver = new AutoSaver({
      persist: (r) => writeAutosave(storage, r),
      snapshot: () => ({ workbook: `wb${version}`, activeSheet: 1 }),
      onState: (s) => states.push(s),
      delayMs: 500,
      ...timers,
    });
    version = 1;
    saver.markDirty();
    version = 2;
    saver.markDirty(); // resets the timer
    expect(saver.pending).toBe(true);
    timers.advance(400);
    expect(readAutosave(storage)).toBeNull(); // not yet
    timers.advance(200);
    expect(saver.pending).toBe(false);
    expect(readAutosave(storage)).toMatchObject({ workbook: "wb2", activeSheet: 1, savedAt: 1600 });
    expect(states).toEqual(["dirty", "dirty", "saving", "saved"]);
    expect(saver.lastSavedAt).toBe(1600);
  });

  it("flush() writes immediately and reports quota errors", () => {
    const storage = fakeStorage();
    const states: Array<[AutosaveState, string | undefined]> = [];
    const saver = new AutoSaver({
      persist: (r) => writeAutosave(storage, r),
      snapshot: () => ({ workbook: "x", activeSheet: 0 }),
      onState: (s, d) => states.push([s, d]),
      now: () => 42,
    });
    expect(saver.flush()).toBe(true);
    expect(readAutosave(storage)?.savedAt).toBe(42);

    storage.setItem = () => {
      const err = new Error("quota");
      err.name = "QuotaExceededError";
      throw err;
    };
    expect(saver.flush()).toBe(false);
    expect(states.at(-1)).toEqual(["error", "workbook too large for local storage"]);

    saver.disable();
    saver.markDirty();
    expect(saver.pending).toBe(false);
    expect(states.at(-1)?.[0]).toBe("disabled");
  });
});
