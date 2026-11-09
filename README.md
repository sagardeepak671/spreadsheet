# Spreadsheet Application

## Overview

This project is a Rust spreadsheet engine (`spreadsheet-core`) with three front-ends: a terminal CLI, an `eframe`/`egui` desktop GUI, and a browser front-end that runs the same Rust engine through WebAssembly. It provides a fully functional spreadsheet with features similar to modern spreadsheet software, including cell editing, formulas, undo/redo functionality, cut/copy/paste, find and replace, sorting, graphing, and theme customization. The application supports multiple sheets, CSV import/export, and a robust dependency tracking system for formula calculations.

**Live Demo:** [https://sagardeepak671.github.io/spreadsheet/](https://sagardeepak671.github.io/spreadsheet/)

The browser version can be tried directly without installing Rust or any other local dependencies.

## Features

### Core Spreadsheet Functionality
- **Cell Editing**: Supports direct cell editing with numeric, string, and formula inputs.
- **Formulas**: Implements a wide range of operations, including:
  - Arithmetic operations (`+`, `-`, `*`, `/`) between cells or constants.
  - Range functions (`SUM`, `MIN`, `MAX`, `AVG`, `STDEV`).
  - Cell referencing (e.g., `A1=B2`).
  - Sleep function (`SLEEP`) for timing operations.
- **Dependency Tracking**: Automatically recalculates dependent cells when a cell's value changes, with cycle detection to prevent infinite loops.
- **Undo/Redo**: Supports undoing and redoing actions like cell edits, cut/paste, find and replace, and sorting.
- **Cut/Copy/Paste**: Allows cutting, copying, and pasting cell contents, with dependency updates for cut operations.

### User Interface
- **Grid Display**: Displays a scrollable grid with row and column headers, styled with customizable themes.
- **Formula Bar**: Shows the formula or value of the selected cell and allows direct formula input.
- **Status Bar**: Displays the current mode, status messages, execution time, and selected cell information.
- **Menu System**: Provides options for:
  - File operations (Save, Open, Save to CSV).
  - Editing (Cut, Copy, Paste, Undo, Redo, Find and Replace).
  - Visualization (Plot Graph).
  - Customization (Font, Theme).
  - Sheet management (New Sheet, Delete Sheet).
  - Sorting and cell navigation (Scroll to).

### Data Management
- **Multiple Sheets**: Supports creating, switching, and deleting sheets, each with configurable rows and columns.
- **CSV Import/Export**: Import data from CSV files and export sheets to CSV format.
- **Serialization**: Saves and loads entire spreadsheet state (including multiple sheets) in a custom `.290` format using JSON.

### Advanced Features
- **Find and Replace**: Replaces text or values across the sheet with undo support.
- **Sorting**: Sorts a range of cells by a specified column or row, with ascending or descending order.
- **Graphing**: Plots data from two columns as a line graph, with customizable row ranges.
- **Theme Customization**: Supports light and dark themes, with customizable colors for cells, headers, and text.
- **Font Selection**: Allows changing the font used in the UI.

## Project Structure

The repository is a Cargo workspace with one reusable engine crate and two thin
front-ends, plus a TypeScript web project:

```
spreadsheet/
├── Cargo.toml                  # workspace (core, desktop, wasm)
├── crates/
│   ├── core/                   # spreadsheet-core  – the engine (shared by everything)
│   │   └── src/
│   │       ├── sheet_functions.rs     Sheet / Cell / OpCode, dependency graph, recalculation
│   │       ├── calculate_functions.rs SUM/MIN/MAX/AVG/STDEV + binary arithmetic
│   │       ├── parser.rs              command language: execute_command (pure) + parse_command (blocking wrapper)
│   │       ├── dependency_ops.rs      dependency rewiring for cut/paste/undo, range sort (was ui_sheet_functions.rs)
│   │       ├── formula.rs             cell -> formula text ("B2=A1+5")
│   │       ├── csv.rs                 CSV text <-> Sheet
│   │       ├── workbook.rs            Workbook / NamedSheet: sheets, editing ops, history, formatting, charts, JSON save/load
│   │       ├── charts.rs              chart definitions + data extraction from cell values
│   │       └── test.rs                original engine test-suite (unchanged)
│   ├── desktop/                # spreadsheet – egui/eframe GUI (--features gui) + terminal CLI (default)
│   │   ├── assets/fonts/
│   │   └── src/ main.rs, app.rs, ui/{app_impl,menu,sheet_display,utils,themes,fonts}.rs
│   └── wasm/                   # spreadsheet-wasm – wasm-bindgen bridge (WasmWorkbook)
│       ├── src/lib.rs
│       └── tests/web.rs        wasm-bindgen-test suite (runs inside wasm via Node)
└── web/                        # TypeScript (Vite) browser front-end
    ├── index.html
    ├── src/bridge.ts           typed wrapper over the wasm bindings
    ├── src/grid/               virtualized grid: model.ts (pure logic), grid.ts (DOM), sheet-adapter.ts (-> Rust)
    ├── src/app.ts              controller: home/library, menubar, tabs, undo/redo, clipboard, find/replace, sort, files, themes
    ├── src/formula-bar.ts, src/clipboard.ts, src/autosave.ts, src/files.ts, src/tabs.ts, src/main.ts
    ├── src/**/*.test.ts        vitest unit tests
    └── e2e/grid.spec.ts        Playwright browser tests
```

### What is shared

Everything in `crates/core` is used unchanged by the CLI, the desktop GUI and
the browser: the data model, the formula parser, dependency tracking and cycle
detection, recalculation, range functions, formula-text reconstruction, CSV
conversion and the JSON (`.290`) serialization format. The core has no GUI,
filesystem, thread or wall-clock dependencies, so it compiles as-is for
`wasm32-unknown-unknown`.

* `crates/desktop` adds: egui rendering, keyboard/mouse handling, themes,
  fonts, undo/redo + clipboard state, plotting, and `std::fs` wrappers around
  the core's CSV/JSON conversions.
* `crates/wasm` adds: nothing but type conversion (`CoreError` -> JS `Error`,
  snapshots -> JSON strings).
* Cell formatting (`CellFormat`) and chart definitions (`ChartSpec`) are part
  of the `Sheet` and therefore of the `.290` file; both are optional fields so
  older files load unchanged. The desktop GUI does not render them yet.

### File format note

`.290` files are JSON (`Vec<NamedSheet>`). Since the workspace refactor the
per-sheet `buul` range-flag grid is written sparsely
(`{"rows","cols","set":[[r,c],..]}`) instead of as a full boolean matrix —
a blank 1000x1000 sheet used to be ~6 MB. Both layouts are read, so files
written by the original desktop build load in the desktop and the browser;
files written now need this build (or newer) of the desktop.

### Building

```sh
# Rust
cargo test --workspace                        # engine + bridge tests (native)
cargo build --release -p spreadsheet          # terminal CLI:  ./target/release/spreadsheet <rows> <cols>
cargo build --release -p spreadsheet --features gui   # desktop GUI

# wasm + web (needs: rustup target add wasm32-unknown-unknown; cargo install wasm-pack; node >= 20)
wasm-pack build crates/wasm --target web --out-dir ../../web/src/wasm-pkg
wasm-pack test --node crates/wasm             # bridge tests inside wasm
cd web && npm install && npm test && npm run dev
cd web && npx playwright install chromium && npm run test:e2e   # browser end-to-end tests
```

`make wasm`, `make wasm-test`, `make web-test`, `make web-dev` and `make clean`
wrap the commands above.

`tests/parity-cases.json` holds shared formula / reference / range-function /
error / cycle cases that the native core, the wasm build and the browser UI all
execute (see `web/README.md`). The core uses `regex-lite` (ASCII patterns
only), which keeps the wasm engine at ~0.77 MB (~220 KB gzipped).
