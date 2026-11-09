/**
 * End-to-end tests for the application shell: home screen + local library,
 * menubar, themes, and the layout on small screens.
 */

import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, test, type Page } from "@playwright/test";

import { menu, open, sheetNames, typeInto, value, waitForEngine } from "./helpers";

const bg = (page: Page, selector: string) =>
  page.locator(selector).first().evaluate((el) => getComputedStyle(el).backgroundColor);

test.describe("home screen", () => {
  test("first visit shows home; new / close / recents / import CSV", async ({ page }) => {
    await page.goto("/");
    await waitForEngine(page);
    await expect(page.locator("#home")).toBeVisible();
    await expect(page.locator("#app")).toBeHidden();
    await expect(page.locator("#home-recent-list")).toContainText("Nothing yet");

    await page.locator("#home-new").click();
    await expect(page.locator("#app")).toBeVisible();
    await expect(page.locator("#home")).toBeHidden();
    await typeInto(page, "A1", "1");
    await page.locator("#wb-name").fill("First");
    await page.locator("#wb-name").press("Enter");
    await expect(page.locator("#autosave")).toContainText("saved ", { timeout: 5000 });

    // close -> home lists it
    await page.locator("#btn-home").click();
    await expect(page.locator("#home")).toBeVisible();
    await expect(page.locator(".recent-row")).toHaveCount(1);
    await expect(page.locator(".recent-name")).toHaveText("First");
    await expect(page.locator(".recent-meta").first()).toContainText("1 sheet");

    // a second workbook from a CSV file
    const csv = join(mkdtempSync(join(tmpdir(), "ss-")), "sales.csv");
    writeFileSync(csv, "a,b\n1,2\n");
    const [chooser] = await Promise.all([page.waitForEvent("filechooser"), page.locator("#home-import").click()]);
    await chooser.setFiles(csv);
    await expect(page.locator("#app")).toBeVisible();
    await expect(page.locator("#wb-name")).toHaveValue("sales");
    expect(await sheetNames(page)).toEqual(["sales.csv"]);
    expect(await value(page, "B2")).toBe("2");

    await menu(page, "file", "btn-close");
    await expect(page.locator(".recent-row")).toHaveCount(2);
    await page.locator(".recent-name", { hasText: "First" }).click();
    await expect(page.locator("#app")).toBeVisible();
    expect(await value(page, "A1")).toBe("1");

    // deleting from recents
    await page.locator("#btn-home").click();
    page.once("dialog", (d) => d.accept());
    await page.locator(".recent-row", { hasText: "sales" }).locator("button.danger").click();
    await expect(page.locator(".recent-row")).toHaveCount(1);

    // reload while on home stays on home
    await page.reload();
    await waitForEngine(page);
    await expect(page.locator("#home")).toBeVisible();
  });

  test("the menubar exposes the workbook and sheet actions", async ({ page }) => {
    await open(page);
    await page.locator("#menu-edit").click();
    const popup = page.locator(".popup-menu:not(.context-menu)");
    await expect(popup).toBeVisible();
    await expect(popup.locator(".popup-menu-item", { hasText: "Undo" })).toBeDisabled();
    await page.keyboard.press("Escape");
    await expect(popup).toBeHidden();
    await menu(page, "insert", "btn-insert-sheet");
    await expect(page.locator("#dlg-new-sheet")).toBeVisible();
    await page.locator("#dlg-new-sheet button", { hasText: "Cancel" }).click();
    await typeInto(page, "A1", "x");
    await page.locator("#menu-format").click();
    await popup.locator(".popup-menu-item", { hasText: "Bold" }).click();
    await page.locator("#menu-format").click();
    await expect(popup.locator(".popup-menu-item", { hasText: "Bold" })).toHaveAttribute("aria-checked", "true");
    await page.keyboard.press("Escape");
    await menu(page, "view", "btn-shortcuts");
    await expect(page.locator("#dlg-shortcuts")).toBeVisible();
    await expect(page.locator("#shortcuts li").first()).toContainText("move");
  });
});

test.describe("themes", () => {
  test("switching themes recolours the chrome, the grid and charts, and is remembered", async ({ page }) => {
    await open(page);
    await typeInto(page, "A1", "1");
    await typeInto(page, "A2", "3");
    await page.locator("#btn-chart").click();
    await page.locator("#chart-start").fill("A1");
    await page.locator("#chart-end").fill("A2");
    await page.locator("#chart-kind").selectOption("bar");
    await page.locator("#chart-submit").click();

    const before = {
      header: await bg(page, ".ssgrid-hdr"),
      toolbar: await bg(page, ".toolbar"),
      bar: await page.locator(".chart-card rect.ss-chart-bar").first().getAttribute("fill"),
    };
    await menu(page, "view", "theme-midnight");
    await expect(page.locator("html")).toHaveAttribute("data-theme", "midnight");
    const after = {
      header: await bg(page, ".ssgrid-hdr"),
      toolbar: await bg(page, ".toolbar"),
      bar: await page.locator(".chart-card rect.ss-chart-bar").first().getAttribute("fill"),
    };
    expect(after.header).not.toBe(before.header);
    expect(after.toolbar).not.toBe(before.toolbar);
    expect(after.bar).not.toBe(before.bar);
    expect(after.bar).toBe("#22d3ee");
    await expect(page.locator("#status")).toContainText("theme: Midnight");

    // menus and dialogs follow the theme (dark surface)
    await page.locator("#menu-file").click();
    expect(await bg(page, ".popup-menu:not(.context-menu)")).toBe("rgb(18, 26, 48)");
    await page.keyboard.press("Escape");

    await page.reload();
    await waitForEngine(page);
    await expect(page.locator("html")).toHaveAttribute("data-theme", "midnight");

    for (const id of ["dark", "warm", "light"]) {
      await menu(page, "view", `theme-${id}`);
      await expect(page.locator("html")).toHaveAttribute("data-theme", id);
    }
    // the home screen has the theme switch too
    await page.locator("#btn-home").click();
    await page.locator("#home-theme").click();
    await page.locator("#theme-warm").click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", "warm");
  });
});

test.describe("layout", () => {
  for (const [width, height] of [
    [1280, 720],
    [1024, 640],
    [800, 600],
    [390, 720],
  ]) {
    test(`does not overflow at ${width}x${height}`, async ({ page }) => {
      await page.setViewportSize({ width, height });
      await page.goto("/");
      await waitForEngine(page);
      const homeOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
      expect(homeOverflow).toBeLessThanOrEqual(0);
      await page.locator("#home-new").click();
      await expect(page.locator("#app")).toBeVisible();
      await typeInto(page, "A1", "42");
      const overflow = await page.evaluate(() => ({
        x: document.documentElement.scrollWidth - window.innerWidth,
        y: document.documentElement.scrollHeight - window.innerHeight,
      }));
      expect(overflow.x).toBeLessThanOrEqual(0);
      expect(overflow.y).toBeLessThanOrEqual(0);
      // the essentials stay visible
      for (const sel of ["#menu-file", "#btn-undo", "#fmt-bold", "#namebox", ".ssgrid-viewport", ".sheet-tab", "#status"]) {
        await expect(page.locator(sel).first()).toBeVisible();
      }
      const grid = (await page.locator(".ssgrid-viewport").boundingBox())!;
      expect(grid.height).toBeGreaterThan(height * 0.45); // the grid remains the main area
      await expect(page.locator(".ssgrid-cell", { hasText: /^42$/ })).toBeVisible();
      // dialogs fit the viewport
      await page.locator("#btn-chart").click();
      const dlg = (await page.locator("#dlg-chart").boundingBox())!;
      expect(dlg.width).toBeLessThanOrEqual(width);
      expect(dlg.height).toBeLessThanOrEqual(height);
      await page.keyboard.press("Escape");
    });
  }
});
