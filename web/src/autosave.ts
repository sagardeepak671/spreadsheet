/**
 * Debounced auto-save: the whole workbook (JSON from the Rust core, same
 * format as `.290` files) is handed to a `persist` callback shortly after
 * every change (see `library.ts` for the localStorage side).
 *
 * Pure with respect to the browser: storage and the clock are injected so the
 * logic is unit-tested with fakes.
 */

export const AUTOSAVE_KEY = "spreadsheet.autosave.v1";

export interface AutosaveRecord {
  /** Format marker for future migrations. */
  version: 1;
  savedAt: number;
  activeSheet: number;
  /** `Workbook::to_json()` output (Vec<NamedSheet>). */
  workbook: string;
}

export type AutosaveState = "idle" | "dirty" | "saving" | "saved" | "error" | "disabled";

export interface KeyValueStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

export function readAutosave(storage: KeyValueStorage): AutosaveRecord | null {
  const raw = storage.getItem(AUTOSAVE_KEY);
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw) as Partial<AutosaveRecord>;
    if (parsed.version !== 1 || typeof parsed.workbook !== "string") return null;
    return {
      version: 1,
      savedAt: typeof parsed.savedAt === "number" ? parsed.savedAt : 0,
      activeSheet: typeof parsed.activeSheet === "number" ? parsed.activeSheet : 0,
      workbook: parsed.workbook,
    };
  } catch {
    return null;
  }
}

export function writeAutosave(storage: KeyValueStorage, record: AutosaveRecord): void {
  storage.setItem(AUTOSAVE_KEY, JSON.stringify(record));
}

export function clearAutosave(storage: KeyValueStorage): void {
  storage.removeItem(AUTOSAVE_KEY);
}

export interface AutoSaverOptions {
  /** Writes a snapshot somewhere persistent (throws on quota errors). */
  persist: (record: AutosaveRecord) => void;
  /** Produces the current workbook JSON + active sheet. */
  snapshot: () => { workbook: string; activeSheet: number };
  onState: (state: AutosaveState, detail?: string) => void;
  /** Quiet time after the last change before saving. */
  delayMs?: number;
  now?: () => number;
  setTimer?: (fn: () => void, ms: number) => unknown;
  clearTimer?: (handle: unknown) => void;
}

/** Debounced writer with a small state machine for the status label. */
export class AutoSaver {
  private timer: unknown = null;
  private readonly delayMs: number;
  private readonly now: () => number;
  private readonly setTimer: (fn: () => void, ms: number) => unknown;
  private readonly clearTimer: (handle: unknown) => void;
  private enabled = true;
  lastSavedAt: number | null = null;

  constructor(private readonly opts: AutoSaverOptions) {
    this.delayMs = opts.delayMs ?? 800;
    this.now = opts.now ?? (() => Date.now());
    this.setTimer = opts.setTimer ?? ((fn, ms) => setTimeout(fn, ms));
    this.clearTimer = opts.clearTimer ?? ((h) => clearTimeout(h as ReturnType<typeof setTimeout>));
  }

  /** Call after every change; the save happens after `delayMs` of quiet. */
  markDirty(): void {
    if (!this.enabled) return;
    this.opts.onState("dirty");
    if (this.timer !== null) this.clearTimer(this.timer);
    this.timer = this.setTimer(() => {
      this.timer = null;
      this.flush();
    }, this.delayMs);
  }

  /** Saves immediately (used on page hide / before unload). */
  flush(): boolean {
    if (!this.enabled) return false;
    if (this.timer !== null) {
      this.clearTimer(this.timer);
      this.timer = null;
    }
    this.opts.onState("saving");
    try {
      const snap = this.opts.snapshot();
      this.opts.persist({
        version: 1,
        savedAt: this.now(),
        activeSheet: snap.activeSheet,
        workbook: snap.workbook,
      });
      this.lastSavedAt = this.now();
      this.opts.onState("saved");
      return true;
    } catch (err) {
      // Typically QuotaExceededError: the workbook is too large for localStorage.
      this.opts.onState("error", (err as Error).name === "QuotaExceededError" ? "workbook too large for local storage" : (err as Error).message);
      return false;
    }
  }

  get pending(): boolean {
    return this.timer !== null;
  }

  /** Drops a pending save without writing. */
  cancel(): void {
    if (this.timer !== null) this.clearTimer(this.timer);
    this.timer = null;
  }

  /** Stops saving (e.g. while a new workbook is being created). */
  disable(): void {
    this.enabled = false;
    if (this.timer !== null) this.clearTimer(this.timer);
    this.timer = null;
    this.opts.onState("disabled");
  }

  enable(): void {
    this.enabled = true;
    this.opts.onState("idle");
  }
}
