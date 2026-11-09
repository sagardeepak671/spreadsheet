/**
 * Boot: load the wasm engine, then hand over to the application controller.
 */

import "./style.css";
import { App } from "./app";
import { bridgeVersion, loadSpreadsheetWasm } from "./bridge";
import { isMacPlatform } from "./grid/model";

async function main(): Promise<void> {
  await loadSpreadsheetWasm();
  const isMac = isMacPlatform(navigator);
  const boot = document.getElementById("boot");
  if (boot) boot.textContent = `engine: spreadsheet-core via wasm v${bridgeVersion()} · ${isMac ? "⌘" : "Ctrl"} shortcuts`;
  new App(isMac);
}

main().catch((err) => {
  const boot = document.getElementById("boot");
  if (boot) boot.textContent = `failed to load wasm: ${(err as Error).message}`;
  console.error(err);
});
