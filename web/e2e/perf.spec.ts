/**
 * Performance smoke: larger sheets and bulk data through the real UI.
 * Prints timings (see the test output) and only fails on generous limits.
 * Run alone (`npm run test:perf`): frame timings are meaningless while
 * other browsers compete for the CPU.
 */

import { expect, test } from "@playwright/test";

import { goto, open } from "./helpers";

const ROWS = 1000;
const COLS = 12;

async function timed<T>(label: string, fn: () => Promise<T>): Promise<{ ms: number; result: T }> {
  const t0 = Date.now();
  const result = await fn();
  const ms = Date.now() - t0;
  console.log(`[perf] ${label}: ${ms} ms`);
  return { ms, result };
}

test("bulk data, formula chains and scrolling stay responsive", async ({ page }) => {
  test.setTimeout(180_000);
  await open(page);

  // 1. bulk paste: ROWS x COLS numbers (one undo step, one engine call)
  const block = Array.from({ length: ROWS }, (_, r) => Array.from({ length: COLS }, (_, c) => String((r * 7 + c * 13) % 1000)));
  const paste = await timed(`paste ${ROWS}x${COLS} values (engine)`, () =>
    page.evaluate((b) => {
      const t = performance.now();
      const ok = window.spreadsheet.wb().pasteTextBlock(0, 0, 0, b).ok;
      return { ok, ms: performance.now() - t };
    }, block),
  );
  expect(paste.result.ok).toBe(true);
  console.log(`[perf]   engine-only time: ${paste.result.ms.toFixed(0)} ms (${((ROWS * COLS) / paste.result.ms).toFixed(0)} cells/ms)`);
  const render = await timed("first render after bulk paste", () =>
    page.evaluate(() => {
      const t = performance.now();
      window.spreadsheet.grid.forceRender();
      return performance.now() - t;
    }),
  );
  console.log(`[perf]   render-only: ${render.result.toFixed(1)} ms, DOM cells ${await page.evaluate(() => window.spreadsheet.grid.stats.renderedCells)}`);
  expect(render.result).toBeLessThan(200);

  // 2. formula column + range functions over the whole column (ROWS dependents of A)
  const formulas = Array.from({ length: ROWS }, (_, r) => [`=A${r + 1}+B${r + 1}`]);
  const chain = await timed(`paste ${ROWS} row formulas =A+B (engine)`, () =>
    page.evaluate((f) => {
      const t = performance.now();
      const ok = window.spreadsheet.wb().pasteTextBlock(0, 0, 12, f).ok; // column M
      return { ok, ms: performance.now() - t };
    }, formulas),
  );
  expect(chain.result.ok).toBe(true);
  await page.evaluate(() => {
    const wb = window.spreadsheet.wb();
    wb.editCell(0, "N1", "=SUM(A1:A1000)");
    wb.editCell(0, "N2", "=MAX(M1:M1000)");
    wb.editCell(0, "N3", "=AVG(B1:B1000)");
    wb.editCell(0, "N4", "=STDEV(C1:C1000)");
    window.spreadsheet.grid.forceRender();
  });

  // 3. editing a cell that 1000 formulas + 2 range functions depend on, through the
  //    grid's commit path (instrumented in-page: Playwright's own typing latency is
  //    not the app's)
  const before = await page.evaluate(() => window.spreadsheet.wb().getDisplayValue(0, "N1"));
  const edit = await page.evaluate(() => {
    const wb = window.spreadsheet.wb();
    const orig = wb.editCell.bind(wb);
    let engine = 0;
    wb.editCell = (...args: Parameters<typeof orig>) => {
      const t = performance.now();
      const res = orig(...args);
      engine += performance.now() - t;
      return res;
    };
    const grid = window.spreadsheet.grid;
    const t0 = performance.now();
    grid.setSelection({ row: 0, col: 0 });
    grid.beginEdit(true, "5000");
    grid.commitEdit("down");
    const total = performance.now() - t0;
    wb.editCell = orig;
    return { total, engine };
  });
  console.log(`[perf] grid commit of A1 with 1000 dependents + SUM/MAX ranges: ${edit.total.toFixed(1)} ms total, engine ${edit.engine.toFixed(1)} ms`);
  await expect(page.locator("#status")).toContainText("edit A1: ok");
  const after = await page.evaluate(() => window.spreadsheet.wb().getDisplayValue(0, "N1"));
  expect(Number(after) - Number(before)).toBe(5000 - Number(block[0][0]));
  expect(edit.engine).toBeLessThan(100);
  expect(edit.total).toBeLessThan(500);

  // 4. scrolling through the filled sheet: per-frame cost of the virtualized grid
  const frames = await page.evaluate(async () => {
    const el = document.querySelector(".ssgrid-viewport") as HTMLElement;
    const times: number[] = [];
    for (let i = 0; i < 80; i++) {
      const t0 = performance.now();
      el.scrollTop = (i * 26 * 9) % (1000 * 26);
      el.scrollLeft = (i * 100 * 3) % (14 * 100);
      await new Promise((r) => requestAnimationFrame(r));
      await new Promise((r) => requestAnimationFrame(r));
      times.push(performance.now() - t0);
    }
    times.sort((a, b) => a - b);
    return { median: times[40], p95: times[76], max: times[79] };
  });
  console.log(`[perf] scroll+render (2 rAF): median ${frames.median.toFixed(1)} ms, p95 ${frames.p95.toFixed(1)} ms, max ${frames.max.toFixed(1)} ms`);
  expect(frames.p95).toBeLessThan(250); // was 3.6 s before will-change was removed from cells
  await goto(page, "M1000");
  await expect(page.locator(".ssgrid-cell", { hasText: /^\d+$/ }).first()).toBeVisible();

  // 5. undo the bulk formula paste, and the size/time of the auto-save snapshot
  const undo = await timed("undo 1000-cell paste (engine)", () =>
    page.evaluate(() => {
      const t = performance.now();
      window.spreadsheet.wb().undo();
      return performance.now() - t;
    }),
  );
  expect(undo.result).toBeLessThan(5000);
  const snapshot = await page.evaluate(() => {
    const t = performance.now();
    const json = window.spreadsheet.wb().toJson();
    return { ms: performance.now() - t, bytes: json.length };
  });
  console.log(`[perf] workbook JSON (auto-save): ${(snapshot.bytes / 1024).toFixed(0)} KB in ${snapshot.ms.toFixed(1)} ms`);
  expect(snapshot.bytes).toBeLessThan(4 * 1024 * 1024); // fits localStorage comfortably
  await expect(page.locator("#autosave")).toContainText("saved ", { timeout: 10_000 });
});

test("a 1000 x 18278 sheet (the engine maximum) creates and scrolls", async ({ page }) => {
  await open(page);
  const create = await timed("create 1000x18278 sheet", async () => {
    await page.locator(".sheet-tab-add").click();
    await page.locator("#new-name").fill("Max");
    await page.locator("#new-rows").fill("1000");
    await page.locator("#new-cols").fill("18278");
    await page.locator("#new-sheet-create").click();
    await expect(page.locator("#sel-info")).toContainText("18278");
  });
  expect(create.ms).toBeLessThan(5000);
  await page.keyboard.press(process.platform === "darwin" ? "Meta+End" : "Control+End");
  await expect(page.locator("#namebox")).toHaveValue("ZZZ1000");
  await page.keyboard.type("1");
  await page.keyboard.press("Enter");
  await expect(page.locator("#status")).toContainText("edit ZZZ1000: ok");
  const cells = await page.evaluate(() => document.querySelectorAll(".ssgrid-cell").length);
  console.log(`[perf] DOM cells at the far corner of 18M virtual cells: ${cells}`);
  expect(cells).toBeLessThan(2000);
});
