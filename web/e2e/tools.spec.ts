/**
 * End-to-end tests for the formatting toolbar, the context menu and charts.
 * Formatting and chart definitions are read back from the Rust workbook to
 * prove they belong to the workbook, not to the screen.
 */

import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";

import { addSheet, cell, goto, input, menu, mod, open, saveWorkbook, typeInto, value, waitForEngine } from "./helpers";

const format = (page: Page, ref: string, sheet = 0) =>
  page.evaluate(
    ([s, r]) => (window as unknown as { spreadsheet: { wb(): { getFormat(s: number, r: string): object } } }).spreadsheet.wb().getFormat(s as number, r as string),
    [sheet, ref],
  );
const charts = (page: Page, sheet = 0) =>
  page.evaluate(
    (s) => (window as unknown as { spreadsheet: { wb(): { charts(s: number): Array<{ id: number; kind: string; title: string }> } } }).spreadsheet.wb().charts(s),
    sheet,
  );

async function setColor(page: Page, id: string, color: string): Promise<void> {
  // <input type=color> cannot be typed into; dispatch the event the toolbar listens to.
  await page.evaluate(
    ([elId, c]) => {
      const el = document.getElementById(elId) as HTMLInputElement;
      el.value = c;
      el.dispatchEvent(new Event("input", { bubbles: true }));
    },
    [id, color],
  );
}

test.describe("formatting", () => {
  test("toolbar formatting is stored in the workbook, painted in the grid and undoable", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "12");
    await typeInto(page, "B1", "=A1*2");
    await goto(page, "A1");
    await page.keyboard.press("Shift+ArrowRight"); // A1:B1
    await page.locator("#fmt-bold").click();
    await page.locator("#fmt-center").click();
    await setColor(page, "fmt-fill", "#ffee88");
    await setColor(page, "fmt-color", "#aa0000");
    await page.locator("#fmt-size").selectOption("18");
    await expect(page.locator("#status")).toContainText("format 2 cell(s): ok");
    expect(await format(page, "A1")).toEqual({ bold: true, fontSize: 18, color: "#aa0000", fill: "#ffee88", align: "center" });
    expect(await format(page, "B1")).toEqual({ bold: true, fontSize: 18, color: "#aa0000", fill: "#ffee88", align: "center" });
    expect(await format(page, "C1")).toEqual({});

    const a1 = cell(page, "12");
    await expect(a1).toHaveCSS("font-weight", "700");
    await expect(a1).toHaveCSS("background-color", "rgb(255, 238, 136)");
    await expect(a1).toHaveCSS("color", "rgb(170, 0, 0)");
    await expect(a1).toHaveCSS("text-align", "center");
    await expect(a1).toHaveCSS("font-size", "18px");

    // the toolbar mirrors the active cell; values / formulas are untouched
    await expect(page.locator("#fmt-bold")).toHaveAttribute("aria-pressed", "true");
    await goto(page, "C1");
    await expect(page.locator("#fmt-bold")).toHaveAttribute("aria-pressed", "false");
    expect(await value(page, "B1")).toBe("24");
    await typeInto(page, "A1", "20");
    expect(await value(page, "B1")).toBe("40");
    await expect(cell(page, "40")).toHaveCSS("font-weight", "700"); // format survived the edit

    // clearing single properties and everything
    await goto(page, "A1");
    await page.locator("#fmt-fill-clear").click();
    expect(await format(page, "A1")).toEqual({ bold: true, fontSize: 18, color: "#aa0000", align: "center" });
    await page.locator("#fmt-clear").click();
    expect(await format(page, "A1")).toEqual({});
    await expect(cell(page, "20")).toHaveCSS("font-weight", "400");

    // undo restores formatting without touching the value
    await page.keyboard.press(`${mod}+z`);
    expect(await format(page, "A1")).toMatchObject({ bold: true });
    expect(await value(page, "A1")).toBe("20");

    // keyboard shortcuts
    await goto(page, "B1");
    await page.keyboard.press(`${mod}+i`);
    await page.keyboard.press(`${mod}+u`);
    expect(await format(page, "B1")).toMatchObject({ italic: true, underline: true });
    await page.keyboard.press(`${mod}+b`);
    expect((await format(page, "B1")) as { bold?: boolean }).not.toHaveProperty("bold");
  });

  test("formatting survives switching sheets, reloading and a .290 round trip", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "hello");
    await goto(page, "A1");
    await page.locator("#fmt-italic").click();
    await setColor(page, "fmt-fill", "#ccffcc");
    await addSheet(page, "Other", 10, 10);
    await typeInto(page, "A1", "x");
    await goto(page, "A1");
    await page.locator("#fmt-underline").click();
    expect(await format(page, "A1", 1)).toEqual({ underline: true });
    expect(await format(page, "A1", 0)).toEqual({ italic: true, fill: "#ccffcc" });

    await page.locator(".sheet-tab", { hasText: "Sheet 1" }).click();
    await expect(cell(page, "hello")).toHaveCSS("font-style", "italic");
    await expect(cell(page, "hello")).toHaveCSS("background-color", "rgb(204, 255, 204)");
    await expect(page.locator("#fmt-italic")).toHaveAttribute("aria-pressed", "true");

    await expect(page.locator("#autosave")).toContainText("saved ", { timeout: 5000 });
    await page.reload();
    await waitForEngine(page);
    expect(await format(page, "A1", 0)).toEqual({ italic: true, fill: "#ccffcc" });
    expect(await format(page, "A1", 1)).toEqual({ underline: true });

    const download = await saveWorkbook(page, "fmt");
    const file = join(mkdtempSync(join(tmpdir(), "ss-")), "fmt.290");
    await download.saveAs(file);
    await menu(page, "file", "btn-new");
    expect(await format(page, "A1", 0)).toEqual({});
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), menu(page, "file", "btn-open")]);
    await chooser.setFiles(file);
    await expect(page.locator("#status")).toContainText("opened");
    expect(await format(page, "A1", 0)).toEqual({ italic: true, fill: "#ccffcc" });
    await expect(cell(page, "hello")).toHaveCSS("font-style", "italic");
  });

  test("cut / copy / sort carry formatting with the cells", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await open(page);
    await typeInto(page, "A1", "3");
    await typeInto(page, "A2", "1");
    await goto(page, "A1");
    await page.locator("#fmt-bold").click();
    await page.keyboard.press(`${mod}+x`);
    await goto(page, "C1");
    await page.keyboard.press(`${mod}+v`);
    await expect(page.locator("#status")).toContainText("moved cells");
    expect(await format(page, "C1")).toEqual({ bold: true });
    expect(await format(page, "A1")).toEqual({});
    await goto(page, "C1");
    await page.keyboard.press(`${mod}+c`);
    await expect(page.locator("#status")).toContainText("Copied 1 cell(s)");
    await goto(page, "A1");
    await page.keyboard.press(`${mod}+v`);
    await expect(page.locator("#status")).toContainText("pasted values");
    expect(await format(page, "A1")).toEqual({ bold: true });
    expect(await value(page, "A1")).toBe("3");
    await page.locator("#formula").fill("SORT(A1:A2;A;asc)");
    await page.locator("#formula").press("Enter");
    expect(await value(page, "A2")).toBe("3");
    expect(await format(page, "A2")).toEqual({ bold: true });
    expect(await format(page, "A1")).toEqual({});
  });
});

test.describe("context menu", () => {
  test("offers cell actions for the clicked cell", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await open(page);
    await typeInto(page, "B2", "77");
    const viewport = page.locator(".ssgrid-viewport");
    const box = (await viewport.boundingBox())!;
    await page.mouse.click(box.x + 150, box.y + 39, { button: "right" }); // B2
    await expect(page.locator(".context-menu")).toBeVisible();
    await expect(page.locator("#namebox")).toHaveValue("B2");
    await expect(page.locator(".context-menu .popup-menu-item", { hasText: "Sort range" })).toBeDisabled(); // single cell
    await page.locator(".context-menu .popup-menu-item", { hasText: "Copy" }).click();
    await expect(page.locator(".context-menu")).toBeHidden();
    await expect(page.locator("#status")).toContainText("Copied 1 cell(s)");

    await page.mouse.click(box.x + 350, box.y + 39, { button: "right" }); // D2
    await page.locator(".context-menu .popup-menu-item", { hasText: "Paste" }).click();
    await expect(page.locator("#status")).toContainText("pasted values"); // paste is async (clipboard)
    expect(await value(page, "D2")).toBe("77");

    await page.mouse.click(box.x + 350, box.y + 39, { button: "right" });
    await page.locator(".context-menu .popup-menu-item", { hasText: "Bold" }).click();
    await expect(page.locator(".context-menu")).toBeHidden();
    expect(await format(page, "D2")).toEqual({ bold: true });

    await page.mouse.click(box.x + 350, box.y + 39, { button: "right" });
    await page.locator(".context-menu .popup-menu-item", { hasText: "Clear formatting" }).click();
    expect(await format(page, "D2")).toEqual({});

    await page.mouse.click(box.x + 350, box.y + 39, { button: "right" });
    await page.locator(".context-menu .popup-menu-item", { hasText: "Clear contents" }).click();
    expect(await input(page, "D2")).toBe("");

    // Escape closes it; right-click inside a selection keeps the selection
    await goto(page, "A1");
    await page.keyboard.press("Shift+ArrowDown");
    await page.keyboard.press("Shift+ArrowRight");
    await page.mouse.click(box.x + 150, box.y + 39, { button: "right" }); // B2 is inside A1:B2
    await expect(page.locator("#sel-info")).toContainText("A1:B2 (4 cells)");
    await expect(page.locator(".context-menu .popup-menu-item", { hasText: "Sort range" })).toBeEnabled();
    await page.keyboard.press("Escape");
    await expect(page.locator(".context-menu")).toBeHidden();

    await page.mouse.click(box.x + 150, box.y + 39, { button: "right" });
    await page.locator(".context-menu .popup-menu-item", { hasText: "Insert chart" }).click();
    await expect(page.locator("#dlg-chart")).toBeVisible();
    await expect(page.locator("#chart-start")).toHaveValue("A1");
    await expect(page.locator("#chart-end")).toHaveValue("B2");
    await page.locator("#dlg-chart button", { hasText: "Cancel" }).click();
  });
});

test.describe("charts", () => {
  async function seedTable(page: Page): Promise<void> {
    const rows = [
      ["Month", "Sales", "Cost"],
      ["Jan", "120", "80"],
      ["Feb", "150", "90"],
      ["Mar", "90", "70"],
    ];
    for (let r = 0; r < rows.length; r++) for (let c = 0; c < 3; c++) await typeInto(page, `${"ABC"[c]}${r + 1}`, rows[r][c]);
    await typeInto(page, "D1", "Profit");
    for (let r = 2; r <= 4; r++) await typeInto(page, `D${r}`, `=B${r}-C${r}`);
  }

  test("insert a chart from a selection with a live preview; it follows edits and formulas", async ({ page }) => {
    test.slow(); // seeds a 4x4 table cell by cell before the chart work
    await open(page);
    await seedTable(page);
    await goto(page, "A1");
    for (let i = 0; i < 3; i++) await page.keyboard.press("Shift+ArrowDown");
    for (let i = 0; i < 3; i++) await page.keyboard.press("Shift+ArrowRight");
    await page.locator("#btn-chart").click();
    await expect(page.locator("#dlg-chart")).toBeVisible();
    await expect(page.locator("#chart-start")).toHaveValue("A1");
    await expect(page.locator("#chart-end")).toHaveValue("D4");
    // preview is live: the text column produces a warning until it is used as X
    await expect(page.locator("#chart-preview svg")).toBeVisible();
    await page.locator("#chart-headers").check();
    await page.locator("#chart-firstx").check();
    await page.locator("#chart-title").fill("Monthly");
    await page.locator("#chart-kind").selectOption("bar");
    await page.locator("#chart-xlabel").fill("Month");
    await page.locator("#chart-ylabel").fill("Amount");
    const preview = page.locator("#chart-preview svg");
    await expect(preview).toHaveClass(/ss-chart-bar/);
    await expect(preview.locator("rect.ss-chart-bar")).toHaveCount(9); // 3 series x 3 months
    await expect(preview.locator("text", { hasText: "Monthly" })).toBeVisible();
    await expect(preview.locator("text", { hasText: "Profit" })).toBeVisible(); // header label from B1:D1
    await expect(preview.locator("text", { hasText: /^Feb$/ })).toBeVisible(); // categories from column A
    await expect(page.locator("#chart-warnings")).toHaveText("");

    await page.locator("#chart-submit").click();
    await expect(page.locator("#dlg-chart")).toBeHidden();
    await expect(page.locator("#chart-panel")).toBeVisible();
    await expect(page.locator("#charts-count")).toHaveText("1");
    const card = page.locator(".chart-card");
    await expect(card).toHaveCount(1);
    await expect(card.locator(".chart-card-title")).toHaveText("Monthly");
    await expect(card.locator(".chart-card-range")).toHaveText("A1:D4");
    expect(await charts(page)).toMatchObject([{ id: 1, kind: "bar", title: "Monthly" }]);

    // editing data re-renders the chart from Rust values (Profit is a formula)
    const profitBar = card.locator("rect.ss-chart-bar").nth(8); // Profit / Mar
    const before = await profitBar.getAttribute("height");
    await typeInto(page, "B4", "300");
    expect(await value(page, "D4")).toBe("230");
    await expect.poll(async () => card.locator("rect.ss-chart-bar").nth(8).getAttribute("height")).not.toBe(before);
    await expect(card.locator("title", { hasText: "Profit: 230" })).toHaveCount(1);

    // edit: switch to a line chart; the chart range button selects the data
    await card.locator("button[aria-label=\"Edit\"]").click();
    await expect(page.locator("#chart-heading")).toHaveText("Edit chart");
    await page.locator("#chart-kind").selectOption("line");
    await page.locator("#chart-submit").click();
    await expect(card.locator("svg.ss-chart")).toHaveClass(/ss-chart-line/);
    expect(await charts(page)).toMatchObject([{ id: 1, kind: "line" }]);
    await card.locator(".chart-card-range").click();
    await expect(page.locator("#sel-info")).toContainText("A1:D4 (16 cells)");

    // charts belong to the sheet: another sheet has none, and they survive a reload
    await addSheet(page, "Empty", 5, 5);
    await expect(page.locator("#charts-count")).toBeHidden();
    await expect(page.locator(".chart-card")).toHaveCount(0);
    await page.locator(".sheet-tab", { hasText: "Sheet 1" }).click();
    await expect(page.locator(".chart-card")).toHaveCount(1);
    await expect(page.locator("#autosave")).toContainText("saved ", { timeout: 5000 });
    await page.reload();
    await waitForEngine(page);
    expect(await charts(page)).toMatchObject([{ id: 1, kind: "line", title: "Monthly" }]);
    await page.locator("#btn-charts").click();
    await expect(page.locator(".chart-card svg.ss-chart-line")).toBeVisible();

    page.once("dialog", (d) => d.accept());
    await page.locator(".chart-card button[aria-label=\"Delete\"]").click();
    expect(await charts(page)).toEqual([]);
    await expect(page.locator("#chart-panel")).toContainText("No charts on this sheet yet");
  });

  test("scatter with numeric x and rows as series; invalid ranges are explained", async ({ page }) => {
    await open(page);
    for (const [ref, text] of [["A1", "1"], ["B1", "4"], ["C1", "9"], ["A2", "2"], ["B2", "8"], ["C2", "18"]]) await typeInto(page, ref, text);
    await goto(page, "A1");
    await page.locator("#btn-chart").click();
    await page.locator("#chart-start").fill("A1");
    await page.locator("#chart-end").fill("C2");
    await page.locator("#chart-kind").selectOption("scatter");
    await page.locator("#chart-rows").check();
    await page.locator("#chart-firstx").check();
    const preview = page.locator("#chart-preview svg");
    await expect(preview).toHaveClass(/ss-chart-scatter/);
    await expect(preview.locator("circle.ss-chart-point")).toHaveCount(3);
    await expect(preview.locator("title", { hasText: "Row 2: 18" })).toHaveCount(1);
    await page.locator("#chart-end").fill("nope");
    await expect(page.locator("#chart-error")).toContainText("valid range");
    await page.locator("#chart-submit").click();
    await expect(page.locator("#dlg-chart")).toBeVisible(); // not inserted
    await page.locator("#chart-end").fill("A1");
    await expect(page.locator("#chart-warnings")).toContainText("only one series");
    await page.locator("#dlg-chart button", { hasText: "Cancel" }).click();
    expect(await charts(page)).toEqual([]);
  });
});
