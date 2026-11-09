# spreadsheet-web

Browser front-end for the Rust `spreadsheet-core` engine (via wasm-bindgen).
Every value, formula and error shown on screen is read back from the Rust
workbook; the TypeScript side holds only UI state (selection, scroll, editor
draft, undo history of *inputs*).

```
web/src/bridge.ts             typed wrapper over the wasm-bindgen output (web/src/wasm-pkg, generated)
web/src/grid/model.ts         pure grid logic: geometry, virtual window, selection, key mapping, clipboard text
web/src/grid/sheet-adapter.ts the only gateway grid -> Rust workbook (+ undo/redo by replaying inputs)
web/src/grid/grid.ts          virtualized grid DOM: headers, cell pool, selection overlays, in-cell editor
web/src/formula-bar.ts        name box + formula input + computed value (+ SORT(...) commands)
web/src/app.ts                controller: sheets, commands, dialogs, files, auto-save
web/src/clipboard.ts          desktop cut/copy/paste semantics + system clipboard mirror
web/src/autosave.ts           debounced localStorage auto-save (pure, tested with fakes)
web/src/files.ts              open / download helpers (.290, .csv)
web/src/tabs.ts               sheet tabs
web/src/format.ts             CellFormat -> CSS, toolbar patches (pure)
web/src/format-toolbar.ts     formatting toolbar
web/src/context-menu.ts       popup menus: menubar dropdowns + right-click menu
web/src/charts/render.ts      dependency-free SVG renderer for ChartData (pure, tested)
web/src/charts/dialog.ts      insert / edit chart dialog with live preview
web/src/charts/panel.ts       chart side panel
web/src/home.ts               home / welcome screen (new, open, import, recent workbooks)
web/src/library.ts            local workbook library in localStorage (several workbooks + index)
web/src/theme.ts              4 themes as CSS variable sets (light, dark, midnight, warm) + chart palette
web/src/icons.ts              inline SVG icon set used everywhere
web/src/style.css             the whole UI: themes, menubar, toolbar, grid, tabs, dialogs, home, responsive rules
web/src/main.ts               boot
web/src/*.test.ts             vitest unit tests (bridge, grid model, clipboard, autosave, library, themes, charts, parity)
web/e2e/*.spec.ts             Playwright end-to-end tests in Chromium (grid, workbook, tools, shell, parity, perf)
```

## What it does

| feature (desktop original)        | browser                                                       |
|-----------------------------------|---------------------------------------------------------------|
| grid, formula bar, editing        | virtualized grid (`src/grid/`), same command language          |
| sheets: new / rename / delete     | tabs at the bottom (`+`, double-click, `×`)                    |
| undo / redo                       | ⌘/Ctrl+Z, ⌘/Ctrl+Shift+Z, toolbar; history lives in Rust       |
| cut + paste (references follow)   | ⌘/Ctrl+X then ⌘/Ctrl+V → `Workbook::move_cells`                |
| copy + paste (values only)        | ⌘/Ctrl+C then ⌘/Ctrl+V → `Workbook::copy_values`               |
| paste from other apps             | tab-separated text → `paste_text_block` (editing rules)        |
| find & replace                    | Find / Replace dialog → `Workbook::find` / `find_and_replace`  |
| sort (dialog or `SORT(A1:B5;A;asc)`) | Sort… dialog or the formula bar → `Workbook::sort_range`    |
| Save / Open `.290`                | Save .290 downloads, Open… / drag & drop loads                 |
| Save to CSV / Open CSV            | Export CSV downloads the sheet, Open… adds a CSV as a sheet    |
| (new) auto-save                   | localStorage, debounced; restored on reload; status in toolbar |
| (new) cell formatting             | bold/italic/underline, font size, text & fill colour, alignment; stored per cell in the Rust workbook (`Sheet::formats`), undoable, follows cut/copy/sort, saved in `.290` |
| plot graph (desktop: two columns, line only) | Insert chart… from any range: line / bar / scatter, title + axis labels, series in rows or columns, header names, X column; definitions stored per sheet (`Sheet::charts`), data read live from Rust; chart panel with edit / delete |
| (new) right-click context menu    | cut / copy / paste / clear, insert chart, sort, find, bold, clear formatting |
| themes (desktop: 6 colour sets)   | View › theme: Clean light, Dark, Midnight, Warm — grid, headers, selection, menus, dialogs and charts all follow; remembered per browser |
| (new) home screen + local library | new / open / import CSV / recent workbooks kept on this device; File › Close returns to it |

Not ported (yet): font family selection, vim-style `h/j/k/l` navigation.

## Commands

```sh
npm install
npm run build:wasm      # wasm-pack build ../crates/wasm --target web --out-dir ../../web/src/wasm-pkg
npm run typecheck
npm test                # vitest: TypeScript -> wasm -> Rust (Node)
npx playwright install chromium   # one-time, for the browser tests
npm run test:e2e        # Playwright: real browser, real grid (33 specs incl. the shared parity cases)
npm run test:perf       # bulk data / large sheet timings, run alone
npm run build           # production bundle in dist/ (~0.9 MB, ~250 KB gzipped)
npm run preview         # serve dist/ locally
npm run dev             # http://localhost:5173
```

## Parity

`../tests/parity-cases.json` is one list of commands and expected values that
is executed by the native Rust core (`cargo test shared_parity`), by the wasm
build in Node (`src/parity.test.ts`) and by the browser UI
(`e2e/parity.spec.ts`); the desktop CLI prints the same final grid.

## Editing rules (implemented in Rust, `Workbook::edit_cell`)

| you type            | stored as                                   |
|---------------------|---------------------------------------------|
| `42`, `-3`          | constant                                    |
| `A1+5`, `SUM(A1:B9)`| formula (same language as the desktop/CLI)  |
| `=A1*2`             | formula, strict (errors are reported)       |
| `hello world`       | text (anything the parser rejects)          |
| `"quoted"`          | text                                        |
| empty / Delete      | cell cleared, dependents recalculated       |

Untouched cells are blank in the grid but are `0` for the engine (so
`SUM` over them works and a referenced-but-empty cell shows `0`).
