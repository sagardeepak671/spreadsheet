/**
 * The shared parity cases typed into the real grid: every command is entered
 * as `=<rhs>` through the cell editor and every expected value is read from
 * the rendered cell (and the Rust workbook behind it).
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test } from "@playwright/test";

import { addSheet, goto, open, value } from "./helpers";

interface Step {
  cmd?: string;
  status?: string;
  sleepSeconds?: number;
  check?: Record<string, string>;
}

const cases = JSON.parse(readFileSync(fileURLToPath(new URL("../../tests/parity-cases.json", import.meta.url)), "utf8")) as {
  rows: number;
  cols: number;
  steps: Step[];
};

test("parity: the browser UI gives the same results as the native core", async ({ page }) => {
  test.setTimeout(120_000);
  await open(page);
  // the cases assume the 10x10 sheet the native test uses (Z99 must be out of range)
  await addSheet(page, "Parity", cases.rows, cases.cols);
  const sheet = 1;
  let checks = 0;
  for (const [i, step] of cases.steps.entries()) {
    if (step.cmd !== undefined) {
      const eq = step.cmd.indexOf("=");
      const ref = step.cmd.slice(0, eq);
      const rhs = step.cmd.slice(eq + 1);
      await goto(page, ref);
      await page.keyboard.type(`=${rhs}`);
      await page.keyboard.press("Enter");
      if (step.status === "ok") {
        await expect(page.locator("#status"), `step ${i} ${step.cmd}`).toContainText(`edit ${ref}: ok`);
        if (step.sleepSeconds) await expect(page.locator("#status")).toContainText(`SLEEP ${step.sleepSeconds}s requested`);
      } else {
        await expect(page.locator("#status"), `step ${i} ${step.cmd}`).toContainText(step.status!);
        await page.keyboard.press("Escape"); // rejected edits keep the editor open
      }
    } else if (step.check) {
      for (const [ref, expected] of Object.entries(step.check)) {
        expect(await value(page, ref, sheet), `step ${i} cell ${ref}`).toBe(expected);
        await goto(page, ref);
        // the rendered cell text (DOM) matches the engine value ("" for blanks)
        await expect(page.locator("#value")).toHaveText(expected === "0" ? /^0?$/ : expected);
        checks++;
      }
    }
  }
  expect(checks).toBeGreaterThan(50);
});
