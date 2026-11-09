//! # spreadsheet-core
//!
//! The reusable spreadsheet engine shared by every front-end:
//!
//! * `crates/desktop` – egui/eframe GUI + terminal CLI (native only)
//! * `crates/wasm`    – wasm-bindgen bridge for the browser
//!
//! Design rules for this crate:
//!
//! * **No GUI types** (no egui / eframe).
//! * **No filesystem, no threads, no wall-clock** inside the engine. Anything
//!   that must block, sleep or touch files lives in the host crate. The only
//!   exception is [`parser::parse_command`], the legacy blocking wrapper that
//!   the CLI / desktop app still call; the wasm bridge uses
//!   [`parser::execute_command`] / [`workbook::Workbook`] instead.
//! * Must compile for `wasm32-unknown-unknown` without feature flags.
//!
//! Module map:
//!
//! | module                | contents                                                      |
//! |-----------------------|---------------------------------------------------------------|
//! | `sheet_functions`     | `Sheet`, `Cell`, `OpCode`, dependency graph, recalculation     |
//! | `calculate_functions` | SUM/MIN/MAX/AVG/STDEV and binary arithmetic                    |
//! | `parser`              | command language (`A1=B1+5`, `A1=SUM(A1:B2)`, `A1="text"`…)    |
//! | `dependency_ops`      | dependency rewiring used by cut/paste/undo + range sort        |
//! | `formula`             | reconstruct the formula text of a cell (`A1=B1+5`)             |
//! | `csv`                 | CSV text <-> `Sheet`                                           |
//! | `workbook`            | multi-sheet container, cell-reference helpers, JSON save/load  |
//! | `charts`              | chart definitions + data extraction from cell values           |

pub mod calculate_functions;
pub mod charts;
pub mod csv;
pub mod dependency_ops;
pub mod formula;
pub mod parser;
pub mod sheet_functions;
pub mod workbook;

mod test;

pub use sheet_functions::{Align, Cell, CellFormat, CellInfo, OpCode, Sheet};
pub use workbook::{CoreError, NamedSheet, Workbook};
