/**
 * Local workbook library (localStorage): several workbooks, each saved as
 * JSON from the Rust core, plus an index with name / date / size so the home
 * screen can list "recent on this device". Replaces the single auto-save slot
 * of earlier versions (which is migrated on first use).
 */

import { AUTOSAVE_KEY, readAutosave, type KeyValueStorage } from "./autosave";

export const LIBRARY_KEY = "spreadsheet.library.v1";
export const WORKBOOK_KEY_PREFIX = "spreadsheet.workbook.";

export interface WorkbookMeta {
  id: string;
  name: string;
  savedAt: number;
  sheets: number;
  /** Approximate JSON size in bytes. */
  bytes: number;
}

export interface LibraryIndex {
  version: 1;
  /** Workbook opened last; reopened automatically on the next visit. */
  current: string | null;
  items: WorkbookMeta[];
}

export interface StoredWorkbook {
  version: 1;
  activeSheet: number;
  /** `Workbook::to_json()` output. */
  workbook: string;
}

export function newId(now = Date.now()): string {
  return `${now.toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function readIndex(storage: KeyValueStorage): LibraryIndex {
  const raw = storage.getItem(LIBRARY_KEY);
  if (raw) {
    try {
      const parsed = JSON.parse(raw) as Partial<LibraryIndex>;
      if (parsed.version === 1 && Array.isArray(parsed.items)) {
        return { version: 1, current: typeof parsed.current === "string" ? parsed.current : null, items: parsed.items };
      }
    } catch {
      /* fall through: corrupt index is rebuilt */
    }
  }
  return { version: 1, current: null, items: [] };
}

export function writeIndex(storage: KeyValueStorage, index: LibraryIndex): void {
  storage.setItem(LIBRARY_KEY, JSON.stringify(index));
}

export function loadWorkbook(storage: KeyValueStorage, id: string): StoredWorkbook | null {
  const raw = storage.getItem(WORKBOOK_KEY_PREFIX + id);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<StoredWorkbook>;
    if (parsed.version !== 1 || typeof parsed.workbook !== "string") return null;
    return { version: 1, activeSheet: typeof parsed.activeSheet === "number" ? parsed.activeSheet : 0, workbook: parsed.workbook };
  } catch {
    return null;
  }
}

/** Saves a workbook and updates its index entry (most recent first). */
export function saveWorkbook(
  storage: KeyValueStorage,
  meta: Omit<WorkbookMeta, "bytes" | "savedAt"> & { savedAt?: number },
  data: Omit<StoredWorkbook, "version">,
): WorkbookMeta {
  const record: StoredWorkbook = { version: 1, ...data };
  const raw = JSON.stringify(record);
  storage.setItem(WORKBOOK_KEY_PREFIX + meta.id, raw);
  const index = readIndex(storage);
  const entry: WorkbookMeta = { ...meta, savedAt: meta.savedAt ?? Date.now(), bytes: raw.length };
  index.items = [entry, ...index.items.filter((i) => i.id !== meta.id)];
  index.current = meta.id;
  writeIndex(storage, index);
  return entry;
}

export function deleteWorkbook(storage: KeyValueStorage, id: string): void {
  storage.removeItem(WORKBOOK_KEY_PREFIX + id);
  const index = readIndex(storage);
  index.items = index.items.filter((i) => i.id !== id);
  if (index.current === id) index.current = null;
  writeIndex(storage, index);
}

export function renameWorkbook(storage: KeyValueStorage, id: string, name: string): void {
  const index = readIndex(storage);
  const item = index.items.find((i) => i.id === id);
  if (item) {
    item.name = name;
    writeIndex(storage, index);
  }
}

export function setCurrent(storage: KeyValueStorage, id: string | null): void {
  const index = readIndex(storage);
  index.current = id;
  writeIndex(storage, index);
}

/**
 * Imports the single auto-save slot of earlier versions into the library
 * (once) so nothing is lost after upgrading.
 */
export function migrateLegacyAutosave(storage: KeyValueStorage, sheetCount: (json: string) => number): WorkbookMeta | null {
  const legacy = readAutosave(storage);
  if (!legacy) return null;
  let sheets = 0;
  try {
    sheets = sheetCount(legacy.workbook);
  } catch {
    storage.removeItem(AUTOSAVE_KEY);
    return null;
  }
  const meta = saveWorkbook(
    storage,
    { id: newId(), name: "Recovered workbook", sheets, savedAt: legacy.savedAt || Date.now() },
    { activeSheet: legacy.activeSheet, workbook: legacy.workbook },
  );
  storage.removeItem(AUTOSAVE_KEY);
  return meta;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function formatWhen(savedAt: number, now = Date.now()): string {
  const diff = Math.max(0, now - savedAt);
  const min = Math.floor(diff / 60_000);
  if (min < 1) return "just now";
  if (min < 60) return `${min} min ago`;
  const h = Math.floor(min / 60);
  if (h < 24) return `${h} h ago`;
  return new Date(savedAt).toLocaleDateString();
}
