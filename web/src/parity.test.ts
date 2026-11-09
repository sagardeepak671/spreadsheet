/**
 * Runs the shared parity cases (tests/parity-cases.json) through the wasm
 * build of the engine in Node and compares with the expectations that the
 * native Rust test (crates/core) verifies. Same input, same engine bytes
 * compiled for a different target: the results must be identical.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { beforeAll, describe, expect, it } from "vitest";

import { loadSpreadsheetWasmSync, SpreadsheetWorkbook } from "./bridge";

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

beforeAll(() => {
  loadSpreadsheetWasmSync(readFileSync(fileURLToPath(new URL("./wasm-pkg/spreadsheet_wasm_bg.wasm", import.meta.url))));
});

describe("parity: shared cases through wasm", () => {
  it("gives the same statuses and values as the native core", () => {
    const wb = SpreadsheetWorkbook.create(cases.rows, cases.cols);
    let checks = 0;
    cases.steps.forEach((step, i) => {
      if (step.cmd !== undefined) {
        const result = wb.executeCommand(0, step.cmd);
        expect(result.status, `step ${i} \`${step.cmd}\``).toBe(step.status);
        if (step.sleepSeconds !== undefined) expect(result.sleepSeconds, `step ${i} sleep`).toBe(step.sleepSeconds);
      } else if (step.check) {
        for (const [ref, expected] of Object.entries(step.check)) {
          expect(wb.getDisplayValue(0, ref), `step ${i} cell ${ref}`).toBe(expected);
          checks++;
        }
      }
    });
    expect(checks).toBeGreaterThan(50);
    // the same workbook survives a JSON round trip with identical values
    const back = SpreadsheetWorkbook.fromJson(wb.toJson());
    for (const step of cases.steps) {
      if (!step.check) continue;
      for (const [ref, expected] of Object.entries(step.check)) {
        if (step === cases.steps.at(-1)) expect(back.getDisplayValue(0, ref)).toBe(expected);
      }
    }
    wb.free();
    back.free();
  });

  it("the grid editing path (editCell) agrees with the raw command path", () => {
    const raw = SpreadsheetWorkbook.create(cases.rows, cases.cols);
    const edited = SpreadsheetWorkbook.create(cases.rows, cases.cols);
    for (const step of cases.steps) {
      if (step.cmd === undefined) continue;
      const eq = step.cmd.indexOf("=");
      const ref = step.cmd.slice(0, eq);
      const rhs = step.cmd.slice(eq + 1);
      raw.executeCommand(0, step.cmd);
      // `=` prefix = strict formula, exactly what the grid sends for formulas
      const result = edited.editCell(0, ref, `=${rhs}`);
      expect(result.status).toBe(step.status);
    }
    for (let r = 1; r <= cases.rows; r++) {
      for (const c of "ABCDEFGHIJ") {
        const ref = `${c}${r}`;
        expect(edited.getDisplayValue(0, ref), ref).toBe(raw.getDisplayValue(0, ref));
        expect(edited.getFormula(0, ref), ref).toBe(raw.getFormula(0, ref));
      }
    }
    raw.free();
    edited.free();
  });
});
