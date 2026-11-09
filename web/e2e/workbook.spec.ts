/**
 * End-to-end tests for the workbook features ported from the desktop app:
 * sheet tabs, undo/redo, cut/copy/paste, find & replace, sort, file
 * open/download, and the browser auto-save.
 */

import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test } from "@playwright/test";

import { addSheet, formula, goto, input, menu, mod, open, saveWorkbook, sheetNames, tabNames, typeInto, value, waitForEngine } from "./helpers";

test.describe("sheet tabs", () => {
  test("create, switch, rename and delete sheets", async ({ page }) => {
    await open(page);
    await expect(page.locator(".sheet-tab")).toHaveCount(1);
    await typeInto(page, "A1", "1");

    await addSheet(page, "Budget", 20, 5);
    expect(await tabNames(page)).toEqual(["Sheet 1", "Budget"]);
    await expect(page.locator("#sel-info")).toContainText('"Budget" · 20 × 5');
    await typeInto(page, "A1", "2");
    expect(await value(page, "A1", 1)).toBe("2");
    expect(await value(page, "A1", 0)).toBe("1");

    // switch back by clicking the tab; the grid shows that sheet's data
    await page.locator(".sheet-tab", { hasText: "Sheet 1" }).click();
    await expect(page.locator(".sheet-tab.is-active .sheet-tab-name")).toHaveText("Sheet 1");
    await expect(page.locator(".ssgrid-cell", { hasText: /^1$/ })).toBeVisible();
    await expect(page.locator(".ssgrid-cell", { hasText: /^2$/ })).toHaveCount(0);

    // rename inline
    await page.locator(".sheet-tab.is-active .sheet-tab-name").dblclick();
    await page.locator(".sheet-tab-rename").fill("Main");
    await page.locator(".sheet-tab-rename").press("Enter");
    expect(await sheetNames(page)).toEqual(["Main", "Budget"]);
    await expect(page.locator("#status")).toContainText('renamed sheet to "Main"');

    // bad sizes are rejected, valid ranges explained
    await page.locator(".sheet-tab-add").click();
    await page.locator("#new-rows").fill("5000");
    await page.locator("#new-sheet-create").click();
    await expect(page.locator("#new-sheet-error")).toContainText("rows must be 1–1000");
    await page.locator("#dlg-new-sheet button", { hasText: "Cancel" }).click();

    // delete (confirm dialog) – the only remaining sheet cannot be deleted
    page.once("dialog", (d) => d.accept());
    await page.locator(".sheet-tab", { hasText: "Budget" }).locator(".sheet-tab-close").click();
    expect(await sheetNames(page)).toEqual(["Main"]);
    await page.locator(".sheet-tab-close").click();
    await expect(page.locator("#status")).toContainText("cannot delete the only sheet");
    expect(await sheetNames(page)).toEqual(["Main"]);
  });
});

test.describe("editing features from the desktop", () => {
  test("undo / redo works across sheets and through the toolbar", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "5");
    await typeInto(page, "B1", "=A1*2");
    await addSheet(page, "Two", 10, 10);
    await typeInto(page, "A1", "x");
    await expect(page.locator("#btn-undo")).toBeEnabled();
    await expect(page.locator("#btn-redo")).toBeDisabled();

    await page.keyboard.press(`${mod}+z`); // undo "x" on sheet Two
    expect(await input(page, "A1", 1)).toBe("");
    await page.keyboard.press(`${mod}+z`); // undo B1 on sheet 1 -> the UI switches sheet
    await expect(page.locator(".sheet-tab.is-active .sheet-tab-name")).toHaveText("Sheet 1");
    await expect(page.locator("#namebox")).toHaveValue("B1");
    expect(await input(page, "B1")).toBe("");
    await expect(page.locator("#status")).toContainText('undo "edit B1"');

    await page.locator("#btn-redo").click();
    expect(await value(page, "B1")).toBe("10");
    await page.locator("#btn-undo").click();
    await page.locator("#btn-undo").click();
    expect(await input(page, "A1")).toBe("");
    await expect(page.locator("#btn-undo")).toBeDisabled();
    await page.keyboard.press(`${mod}+Shift+z`);
    await page.keyboard.press(`${mod}+y`);
    expect(await value(page, "B1")).toBe("10");
  });

  test("cut + paste moves cells and rewires references; copy + paste pastes values", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await open(page);
    await typeInto(page, "A1", "5");
    await typeInto(page, "B1", "=A1*2");
    await typeInto(page, "C1", "=SUM(A1:A2)");

    await goto(page, "A1");
    await page.keyboard.press(`${mod}+x`);
    await expect(page.locator(".ssgrid-marker")).toBeVisible();
    await goto(page, "D4");
    await page.keyboard.press(`${mod}+v`);
    await expect(page.locator("#status")).toContainText("moved cells");
    expect(await input(page, "D4")).toBe("5");
    expect(await formula(page, "B1")).toBe("B1=D4*2");
    expect(await value(page, "B1")).toBe("10");
    expect(await value(page, "C1")).toBe("0");
    await expect(page.locator(".ssgrid-marker")).toBeHidden();

    await typeInto(page, "D4", "8");
    expect(await value(page, "B1")).toBe("16");

    // copy B1 (a formula) -> E1 gets the value 16, not the formula
    await goto(page, "B1");
    await page.locator("#btn-copy").click();
    await expect(page.locator("#status")).toContainText("Copied 1 cell(s)");
    await goto(page, "E1");
    await page.locator("#btn-paste").click();
    await expect(page.locator("#status")).toContainText("pasted values");
    expect(await input(page, "E1")).toBe("16");
    await typeInto(page, "D4", "1");
    expect(await value(page, "E1")).toBe("16");
    expect(await value(page, "B1")).toBe("2");

    // undo the paste, then the edit, then the move
    await page.keyboard.press(`${mod}+z`); // D4=1
    await page.keyboard.press(`${mod}+z`); // paste values
    expect(await input(page, "E1")).toBe("");
    await page.keyboard.press(`${mod}+z`); // D4=8
    await page.keyboard.press(`${mod}+z`); // move
    expect(await formula(page, "B1")).toBe("B1=A1*2");
    expect(await value(page, "C1")).toBe("5");
    expect(await input(page, "D4")).toBe("");
  });

  test("find next and replace all (constants only, like the desktop)", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "10");
    await typeInto(page, "A2", "=A1");
    await typeInto(page, "B3", "10");
    await typeInto(page, "C1", "hello");

    await page.locator("#btn-find").click();
    await page.locator("#find-text").fill("10");
    await page.locator("#find-next").click();
    await expect(page.locator("#status")).toContainText("match 1 of 3: A1");
    await page.locator("#find-next").click();
    await expect(page.locator("#status")).toContainText("match 2 of 3: A2");
    await page.locator("#find-text").press("Enter");
    await expect(page.locator("#status")).toContainText("match 3 of 3: B3");

    await page.locator("#replace-text").fill("20");
    await page.locator("#replace-all").click();
    await expect(page.locator("#status")).toContainText("Replaced 2 instance(s)");
    expect(await value(page, "A1")).toBe("20");
    expect(await value(page, "B3")).toBe("20");
    expect(await formula(page, "A2")).toBe("A2=A1"); // formula untouched, recalculated
    expect(await value(page, "A2")).toBe("20");

    await page.locator("#find-text").fill("nothing");
    await page.locator("#replace-all").click();
    await expect(page.locator("#status")).toContainText("No matches found");
    await page.locator("#dlg-find button", { hasText: "Close" }).click();
    await page.keyboard.press(`${mod}+z`);
    expect(await value(page, "A1")).toBe("10");
  });

  test("sort via the dialog and via the SORT(...) formula-bar command", async ({ page }) => {
    await open(page);
    for (const [ref, text] of [
      ["A1", "3"], ["B1", "c"],
      ["A2", "1"], ["B2", "a"],
      ["A3", "2"], ["B3", "b"],
      ["D1", "=A1*10"],
    ]) await typeInto(page, ref, text);

    await goto(page, "A1");
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowRight");
    await page.locator("#btn-sort").click();
    await expect(page.locator("#sort-start")).toHaveValue("A1");
    await expect(page.locator("#sort-end")).toHaveValue("B3");
    await page.locator("#sort-key").fill("A");
    await page.locator("#sort-asc").click();
    await expect(page.locator("#status")).toContainText("sort A1:B3 by A asc: ok");
    expect(await Promise.all(["B1", "B2", "B3"].map((r) => value(page, r)))).toEqual(["a", "b", "c"]);
    expect(await value(page, "D1")).toBe("10");

    await page.locator("#formula").fill("SORT(A1:B3;A;desc)");
    await page.locator("#formula").press("Enter");
    await expect(page.locator("#status")).toContainText("SORT(A1:B3;A;desc): ok");
    expect(await Promise.all(["A1", "A2", "A3"].map((r) => value(page, r)))).toEqual(["3", "2", "1"]);

    // validation messages from the engine
    await page.locator("#formula").fill("SORT(A1:D3;A;asc)");
    await page.locator("#formula").press("Enter");
    await expect(page.locator("#status")).toContainText("range has constraints");
    await page.locator("#formula").fill("SORT(A1:B3;B;asc)");
    await page.locator("#formula").press("Enter");
    await expect(page.locator("#status")).toContainText("sorted column has string");

    await page.keyboard.press(`${mod}+z`);
    expect(await Promise.all(["A1", "A2", "A3"].map((r) => value(page, r)))).toEqual(["1", "2", "3"]);
  });
});

test.describe("files", () => {
  test("download .290 and open it again (round trip through the browser)", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "7");
    await typeInto(page, "B1", "=A1*6");
    await addSheet(page, "Notes", 10, 4);
    await typeInto(page, "A1", "remember");

    const download = await saveWorkbook(page, "roundtrip");
    expect(download.suggestedFilename()).toBe("roundtrip.290");
    const dir = mkdtempSync(join(tmpdir(), "ss-"));
    const file = join(dir, "roundtrip.290");
    await download.saveAs(file);
    const text = readFileSync(file, "utf8");
    expect(text).toContain('"name": "Notes"'); // pretty JSON, desktop layout
    expect(JSON.parse(text)).toHaveLength(2);

    // start fresh, then open the file
    await menu(page, "file", "btn-new");
    expect(await sheetNames(page)).toEqual(["Sheet 1"]);
    expect(await input(page, "A1")).toBe("");

    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu(page, "file", "btn-open")]);
    await chooser.setFiles(file);
    await expect(page.locator("#status")).toContainText('opened "roundtrip.290" (2 sheet(s))');
    expect(await sheetNames(page)).toEqual(["Sheet 1", "Notes"]);
    expect(await value(page, "B1")).toBe("42");
    expect(await value(page, "A1", 1)).toBe("remember");
    await typeInto(page, "A1", "1");
    expect(await value(page, "B1")).toBe("6"); // formulas are live after loading
  });

  test("export CSV and import CSV as a new sheet", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "1");
    await typeInto(page, "B1", "=A1+1");
    await typeInto(page, "A2", "hi, there");
    await addSheet(page, "Small", 2, 2);
    await typeInto(page, "A1", "9");
    await typeInto(page, "B2", "=A1*2");

    const [download] = await Promise.all([page.waitForEvent("download"), menu(page, "file", "btn-csv")]);
    expect(download.suggestedFilename()).toBe("Small.csv");
    const dir = mkdtempSync(join(tmpdir(), "ss-"));
    const file = join(dir, "Small.csv");
    await download.saveAs(file);
    expect(readFileSync(file, "utf8")).toBe("9,0\n0,18\n"); // values, not formulas

    const csv = join(dir, "import.csv");
    writeFileSync(csv, '1,2,3\n4,"x, y",6\n');
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu(page, "file", "btn-import")]);
    await chooser.setFiles(csv);
    await expect(page.locator("#status")).toContainText("CSV loaded successfully");
    expect(await sheetNames(page)).toEqual(["Sheet 1", "Small", "import.csv"]);
    await expect(page.locator("#sel-info")).toContainText('"import.csv" · 2 × 3');
    expect(await value(page, "B2", 2)).toBe("x, y");
    expect(await value(page, "C2", 2)).toBe("6");
  });

  test("unsupported and corrupt files are reported", async ({ page }) => {
    await open(page);
    const dir = mkdtempSync(join(tmpdir(), "ss-"));
    const bad = join(dir, "bad.290");
    writeFileSync(bad, "{ definitely not a workbook");
    let [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu(page, "file", "btn-open")]);
    await chooser.setFiles(bad);
    await expect(page.locator("#status")).toContainText("not a valid workbook");
    const txt = join(dir, "notes.txt");
    writeFileSync(txt, "hello");
    [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu(page, "file", "btn-open")]);
    await chooser.setFiles(txt);
    await expect(page.locator("#status")).toContainText("unsupported file");
  });
});

test.describe("auto-save", () => {
  test("the workbook survives a reload; 'New' starts another one and keeps the old in recents", async ({ page }) => {
    await open(page);
    await expect(page.locator("#autosave")).toContainText(/unsaved changes|saved/);
    await typeInto(page, "A1", "123");
    await typeInto(page, "B1", "=A1+1");
    await addSheet(page, "Second", 10, 10);
    await typeInto(page, "A1", "second");
    await page.locator("#wb-name").fill("Budget 2024");
    await page.locator("#wb-name").press("Enter");
    await expect(page.locator("#autosave")).toContainText("saved ", { timeout: 5000 });

    await page.reload();
    await waitForEngine(page);
    await expect(page.locator("#app")).toBeVisible(); // reopened directly, no home screen
    await expect(page.locator("#status")).toContainText('opened "Budget 2024" (2 sheet(s)) from this device');
    await expect(page.locator("#wb-name")).toHaveValue("Budget 2024");
    expect(await sheetNames(page)).toEqual(["Sheet 1", "Second"]);
    await expect(page.locator(".sheet-tab.is-active .sheet-tab-name")).toHaveText("Second"); // active sheet restored
    expect(await value(page, "B1", 0)).toBe("124");
    expect(await value(page, "A1", 1)).toBe("second");
    await page.locator(".sheet-tab", { hasText: "Sheet 1" }).click();
    await typeInto(page, "A1", "5");
    expect(await value(page, "B1", 0)).toBe("6"); // dependencies restored too

    await menu(page, "file", "btn-new");
    await expect(page.locator("#wb-name")).toHaveValue("Untitled");
    expect(await input(page, "A1")).toBe("");
    await expect(page.locator("#autosave")).toContainText("saved ", { timeout: 5000 });
    await page.reload();
    await waitForEngine(page);
    await expect(page.locator("#status")).toContainText('opened "Untitled" (1 sheet(s))');
    expect(await input(page, "A1")).toBe("");

    // both workbooks are listed on the home screen
    await menu(page, "file", "btn-close");
    await expect(page.locator("#home")).toBeVisible();
    await expect(page.locator(".recent-row")).toHaveCount(2);
    await page.locator(".recent-name", { hasText: "Budget 2024" }).click();
    await expect(page.locator("#app")).toBeVisible();
    expect(await value(page, "B1", 0)).toBe("6");
  });
});
