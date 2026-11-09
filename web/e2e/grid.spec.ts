/**
 * End-to-end: real browser -> TypeScript grid -> wasm -> Rust core.
 *
 * Every assertion about a *value* reads it back from the Rust workbook
 * (`window.spreadsheet.wb`) or from the grid DOM, which is itself filled from
 * the workbook, so these tests prove the UI never shows JS-side state.
 */

import { expect, test } from "@playwright/test";

import { activeRef, addSheet, cell, goto, input, mod, open, typeInto, value } from "./helpers";

test.describe("spreadsheet grid", () => {
  test("renders headers, row numbers and a bounded number of DOM cells for a 1000x1000 sheet", async ({ page }) => {
    await open(page);
    await expect(page.locator(".ssgrid-colhdr .ssgrid-hdr").first()).toHaveText("A");
    await expect(page.locator(".ssgrid-rowhdr .ssgrid-hdr").first()).toHaveText("1");
    await expect(page.locator("#sel-info")).toContainText("1000 × 1000");
    const domCells = await page.locator(".ssgrid-cell").count();
    expect(domCells).toBeGreaterThan(50);
    expect(domCells).toBeLessThan(2000);
  });

  test("typing values and formulas shows results computed by the Rust core", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "10");
    await expect(page.locator("#namebox")).toHaveValue("A2"); // Enter moves down
    await page.keyboard.type("20");
    await page.keyboard.press("Tab"); // commit, move right
    await expect(page.locator("#namebox")).toHaveValue("B2");
    await page.keyboard.type("=A1+A2");
    await page.keyboard.press("Enter");
    expect(await value(page, "B2")).toBe("30");
    await expect(cell(page, "30")).toBeVisible();

    await typeInto(page, "C1", "SUM(A1:B2)");
    expect(await value(page, "C1")).toBe("60");
    await typeInto(page, "C2", "hello world");
    expect(await value(page, "C2")).toBe("hello world");
    expect(await input(page, "C2")).toBe('"hello world"');
    await expect(cell(page, "hello world")).toHaveClass(/is-string/);
    await expect(cell(page, "60")).toHaveClass(/has-formula/);
  });

  test("dependents recalculate when a referenced cell changes", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "2");
    await typeInto(page, "B1", "A1*10");
    await typeInto(page, "C1", "B1+1");
    await typeInto(page, "D1", "MAX(A1:C1)");
    expect([await value(page, "B1"), await value(page, "C1"), await value(page, "D1")]).toEqual(["20", "21", "21"]);
    await typeInto(page, "A1", "5");
    expect([await value(page, "B1"), await value(page, "C1"), await value(page, "D1")]).toEqual(["50", "51", "51"]);
    await expect(cell(page, "51").first()).toBeVisible();
    // clearing the source resets the whole chain
    await goto(page, "A1");
    await page.keyboard.press("Delete");
    expect([await value(page, "B1"), await value(page, "C1"), await value(page, "D1")]).toEqual(["0", "1", "1"]);
  });

  test("errors and circular references are reported and leave the cell unchanged", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "0");
    await typeInto(page, "B1", "=7/A1");
    expect(await value(page, "B1")).toBe("Err");
    await expect(cell(page, "Err")).toHaveClass(/is-error/);
    await typeInto(page, "A1", "7");
    expect(await value(page, "B1")).toBe("1");

    await typeInto(page, "A2", "=B2");
    await goto(page, "B2");
    await page.keyboard.type("=A2");
    await page.keyboard.press("Enter");
    await expect(page.locator("#status")).toContainText("circular error");
    // editor stays open with the bad formula; Esc discards it
    await expect(page.locator(".ssgrid-editor")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.locator(".ssgrid-editor")).toBeHidden();
    // B2 is referenced by A2, so the engine materialised it as a plain 0;
    // the rejected circular formula was never stored.
    expect(await input(page, "B2")).toBe("0");
    expect(await value(page, "A2")).toBe("0");

    await typeInto(page, "A3", "=SUM(A1:B1");
    await expect(page.locator("#status")).toContainText("Invalid cmd");
    await page.keyboard.press("Escape");
  });

  test("formula bar shows the active cell's input and value from Rust and can edit it", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "6");
    await typeInto(page, "B1", "A1*7");
    await goto(page, "B1");
    await expect(page.locator("#formula")).toHaveValue("A1*7");
    await expect(page.locator("#value")).toHaveText("42");
    await page.locator("#formula").fill("A1+1");
    await page.locator("#formula").press("Enter");
    expect(await value(page, "B1")).toBe("7");
    await expect(page.locator("#namebox")).toHaveValue("B2");
    // in-cell typing is mirrored live into the formula bar
    await goto(page, "C1");
    await page.keyboard.type("=A1");
    await expect(page.locator("#formula")).toHaveValue("=A1");
    await page.keyboard.press("Escape");
    await expect(page.locator("#formula")).toHaveValue("");
  });

  test("keyboard navigation: arrows, Home/End, Page keys, modifier jumps, F2", async ({ page }) => {
    await open(page);
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowRight");
    await page.keyboard.press("ArrowRight");
    await expect(page.locator("#namebox")).toHaveValue("C2");
    await page.keyboard.press("Home");
    await expect(page.locator("#namebox")).toHaveValue("A2");
    await page.keyboard.press("End");
    await expect(page.locator("#namebox")).toHaveValue("ALL2");
    await page.keyboard.press(`${mod}+Home`);
    await expect(page.locator("#namebox")).toHaveValue("A1");
    await page.keyboard.press("PageDown");
    const afterPage = await activeRef(page);
    expect(Number(afterPage.replace(/^[A-Z]+/, ""))).toBeGreaterThan(10);
    await page.keyboard.press(`${mod}+ArrowDown`);
    await expect(page.locator("#namebox")).toHaveValue("A1000");
    await page.keyboard.press(`${mod}+End`);
    await expect(page.locator("#namebox")).toHaveValue("ALL1000");
    await page.keyboard.type("99");
    await page.keyboard.press("Enter");
    expect(await value(page, "ALL1000")).toBe("99");
    await expect(cell(page, "99")).toBeVisible();
    // the far corner is rendered with the same bounded DOM
    expect(await page.locator(".ssgrid-cell").count()).toBeLessThan(2000);
    await expect(page.locator(".ssgrid-rowhdr .ssgrid-hdr", { hasText: /^1000$/ })).toBeVisible();
    await expect(page.locator(".ssgrid-colhdr .ssgrid-hdr", { hasText: /^ALL$/ })).toBeVisible();

    // F2 edits in place keeping the content
    await page.keyboard.press(`${mod}+Home`);
    await typeInto(page, "A1", "123");
    await goto(page, "A1");
    await page.keyboard.press("F2");
    await expect(page.locator(".ssgrid-editor")).toHaveValue("123");
    await page.keyboard.type("4");
    await page.keyboard.press("Enter");
    expect(await value(page, "A1")).toBe("1234");
  });

  test("scrolling a large sheet virtualizes and keeps values in sync", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "1");
    await typeInto(page, "A500", "A1+499");
    const viewport = page.locator(".ssgrid-viewport");
    await viewport.evaluate((el) => el.scrollTo({ top: 26 * 495, left: 0, behavior: "auto" }));
    await expect(page.locator(".ssgrid-rowhdr .ssgrid-hdr", { hasText: /^500$/ })).toBeVisible();
    await expect(cell(page, "500")).toBeVisible();
    await expect(page.locator("#render-info")).toContainText("rows 49");
    // cells from the top are no longer in the DOM
    expect(await page.locator(".ssgrid-cell", { hasText: /^1$/ }).count()).toBe(0);
    await viewport.evaluate((el) => el.scrollTo({ top: 0, left: 100 * 990, behavior: "auto" }));
    await expect(page.locator(".ssgrid-colhdr .ssgrid-hdr", { hasText: /^ALL$/ })).toBeVisible();
    expect(await page.locator(".ssgrid-cell").count()).toBeLessThan(2000);
    // editing a far-away cell updates a dependent that is currently off-screen
    await goto(page, "A1");
    await page.keyboard.type("10");
    await page.keyboard.press("Enter");
    expect(await value(page, "A500")).toBe("509");
    await goto(page, "A500");
    await expect(cell(page, "509")).toBeVisible();
  });

  test("mouse: click, drag-select, header select, double-click edit", async ({ page }) => {
    await open(page);
    const viewport = page.locator(".ssgrid-viewport");
    const box = (await viewport.boundingBox())!;
    const at = (col: number, row: number) => ({ x: box.x + col * 100 + 50, y: box.y + row * 26 + 13 });

    await page.mouse.click(at(2, 1).x, at(2, 1).y);
    await expect(page.locator("#namebox")).toHaveValue("C2");
    await page.mouse.dblclick(at(2, 1).x, at(2, 1).y);
    await expect(page.locator(".ssgrid-editor")).toBeVisible();
    await page.keyboard.type("77");
    await page.keyboard.press("Enter");
    expect(await value(page, "C2")).toBe("77");

    await page.mouse.move(at(0, 0).x, at(0, 0).y);
    await page.mouse.down();
    await page.mouse.move(at(3, 4).x, at(3, 4).y, { steps: 5 });
    await page.mouse.up();
    await expect(page.locator("#sel-info")).toContainText("A1:D5 (20 cells)");
    await expect(page.locator(".ssgrid-range")).toBeVisible();
    await expect(page.locator(".ssgrid-colhdr .ssgrid-hdr.is-selected")).toHaveCount(4);

    await page.keyboard.press("Delete");
    expect(await value(page, "C2")).toBe("0");
    expect(await input(page, "C2")).toBe("");

    await page.locator(".ssgrid-colhdr .ssgrid-hdr", { hasText: /^B$/ }).click();
    await expect(page.locator("#sel-info")).toContainText("B1:B1000 (1000 cells)");
  });

  test("copy / paste / cut / undo / redo go through the engine", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await open(page);
    await typeInto(page, "A1", "5");
    await typeInto(page, "B1", "A1*2");
    await goto(page, "A1");
    await page.keyboard.press("Shift+ArrowRight");
    await page.keyboard.press(`${mod}+c`);
    await expect(page.locator("#status")).toContainText("Copied 2 cell(s)");
    await goto(page, "A3");
    await page.keyboard.press(`${mod}+v`);
    await expect(page.locator("#status")).toContainText("pasted values");
    expect(await input(page, "A3")).toBe("5");
    expect(await input(page, "B3")).toBe("10"); // desktop copy/paste pastes values, not formulas
    await expect(page.locator("#sel-info")).toContainText("A3:B3 (2 cells)");

    await page.keyboard.press(`${mod}+z`);
    await expect(page.locator("#status")).toContainText("undo");
    expect(await input(page, "A3")).toBe("");
    expect(await input(page, "B3")).toBe("");
    await page.keyboard.press(`${mod}+Shift+z`);
    await expect(page.locator("#status")).toContainText("redo");
    expect(await value(page, "B3")).toBe("10");

    await goto(page, "A1");
    await page.keyboard.press(`${mod}+x`);
    await expect(page.locator("#status")).toContainText("Cut 1 cell(s)");
    await goto(page, "D1");
    await page.keyboard.press(`${mod}+v`);
    await expect(page.locator("#status")).toContainText("moved cells");
    expect(await value(page, "D1")).toBe("5");
    expect(await page.evaluate(() => window.spreadsheet.wb().getFormula(0, "B1"))).toBe("B1=D1*2"); // reference follows
    await page.keyboard.press(`${mod}+z`); // undo move
    expect(await page.evaluate(() => window.spreadsheet.wb().getFormula(0, "B1"))).toBe("B1=A1*2");
    expect(await value(page, "B1")).toBe("10");
  });

  test("name box jumps and rejects bad references; new sheets switch the grid", async ({ page }) => {
    await open(page);
    await goto(page, "zz700");
    await expect(page.locator("#namebox")).toHaveValue("ZZ700");
    await expect(page.locator(".ssgrid-colhdr .ssgrid-hdr", { hasText: /^ZZ$/ })).toBeVisible();
    await page.locator("#namebox").fill("nope");
    await page.locator("#namebox").press("Enter");
    await expect(page.locator("#namebox")).toHaveClass(/is-invalid/);

    await addSheet(page, "Tiny", 20, 5);
    await expect(page.locator("#sel-info")).toContainText('"Tiny" · 20 × 5');
    await expect(page.locator("#namebox")).toHaveValue("A1");
    await page.keyboard.press("End");
    await expect(page.locator("#namebox")).toHaveValue("E1");
    await page.locator(".sheet-tab", { hasText: "Sheet 1" }).click();
    await expect(page.locator("#sel-info")).toContainText("1000 × 1000");
  });
});
