/**
 * Shared helpers for the e2e specs. Values are always read back from the Rust
 * workbook (`window.spreadsheet.wb()`), never from JS-side state.
 */

import { expect, type Page } from "@playwright/test";

declare global {
  interface Window {
    spreadsheet: {
      wb(): {
        getDisplayValue(sheet: number, ref: string): string;
        getInput(sheet: number, ref: string): string;
        getFormula(sheet: number, ref: string): string;
        sheetNames(): string[];
        sheetCount: number;
        canUndo: boolean;
        pasteTextBlock(sheet: number, row: number, col: number, block: string[][]): { ok: boolean };
        editCell(sheet: number, ref: string, text: string): { ok: boolean };
        toJson(): string;
        undo(): unknown;
      };
      grid: {
        stats: { renderedCells: number };
        activeCell: { row: number; col: number };
        forceRender(): void;
        setSelection(pos: { row: number; col: number }): void;
        beginEdit(replace: boolean, text: string): void;
        commitEdit(move: string): boolean;
      };
      app: { activeSheet: number };
    };
  }
}

export const mod = process.platform === "darwin" ? "Meta" : "Control";

/** Waits for the wasm engine, then starts a new workbook from the home screen. */
export async function open(page: Page): Promise<void> {
  await page.goto("/");
  await waitForEngine(page);
  if (await page.locator("#home").isVisible()) await page.locator("#home-new").click();
  await expect(page.locator("#app")).toBeVisible();
  await page.locator(".ssgrid-viewport").focus();
}

export async function waitForEngine(page: Page): Promise<void> {
  await expect(page.locator("#boot")).toContainText("spreadsheet-core", { timeout: 20_000 });
}

/** Clicks an item of a menubar menu (e.g. `menu(page, "file", "btn-save")`). */
export async function menu(page: Page, name: "file" | "edit" | "insert" | "format" | "view", itemId: string): Promise<void> {
  await page.locator(`#menu-${name}`).click();
  await expect(page.locator(`#${itemId}`)).toBeVisible();
  await page.locator(`#${itemId}`).click();
}

/** File › Save as .290 with the given name; resolves with the download. */
export async function saveWorkbook(page: Page, name: string) {
  await menu(page, "file", "btn-save");
  await expect(page.locator("#dlg-save")).toBeVisible();
  await page.locator("#save-name").fill(name);
  const [download] = await Promise.all([page.waitForEvent("download"), page.locator("#save-confirm").click()]);
  return download;
}

export const value = (page: Page, ref: string, sheet = 0): Promise<string> =>
  page.evaluate(([s, r]) => window.spreadsheet.wb().getDisplayValue(s as number, r as string), [sheet, ref]);
export const input = (page: Page, ref: string, sheet = 0): Promise<string> =>
  page.evaluate(([s, r]) => window.spreadsheet.wb().getInput(s as number, r as string), [sheet, ref]);
export const formula = (page: Page, ref: string, sheet = 0): Promise<string> =>
  page.evaluate(([s, r]) => window.spreadsheet.wb().getFormula(s as number, r as string), [sheet, ref]);
export const sheetNames = (page: Page): Promise<string[]> => page.evaluate(() => window.spreadsheet.wb().sheetNames());
export const tabNames = (page: Page): Promise<string[]> => page.locator(".sheet-tab .sheet-tab-name").allTextContents();
export const activeRef = (page: Page): Promise<string> => page.locator("#namebox").inputValue();
export const cell = (page: Page, text: string) => page.locator(".ssgrid-cell", { hasText: new RegExp(`^${text}$`) });

export async function goto(page: Page, ref: string): Promise<void> {
  await page.locator("#namebox").fill(ref);
  await page.locator("#namebox").press("Enter");
  await expect(page.locator("#namebox")).toHaveValue(ref.toUpperCase());
}

export async function typeInto(page: Page, ref: string, text: string, key = "Enter"): Promise<void> {
  await goto(page, ref);
  await page.keyboard.type(text);
  await page.keyboard.press(key);
}

export async function addSheet(page: Page, name: string, rows: number, cols: number): Promise<void> {
  await page.locator(".sheet-tab-add").click();
  await expect(page.locator("#dlg-new-sheet")).toBeVisible();
  await page.locator("#new-name").fill(name);
  await page.locator("#new-rows").fill(String(rows));
  await page.locator("#new-cols").fill(String(cols));
  await page.locator("#new-sheet-create").click();
  await expect(page.locator("#dlg-new-sheet")).toBeHidden();
  await expect(page.locator(".sheet-tab.is-active .sheet-tab-name")).toHaveText(name);
}
