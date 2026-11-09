# Workspace layout
#   crates/core     spreadsheet-core  (engine, shared)
#   crates/desktop  spreadsheet       (egui GUI + terminal CLI)
#   crates/wasm     spreadsheet-wasm  (wasm-bindgen bridge)
#   web/            TypeScript (Vite) browser front-end

all: build

# Native: terminal CLI binary (default features)
build:
	@cargo build --release -p spreadsheet

# All Rust tests: core engine (original suite + workbook/csv tests) and the
# wasm bridge compiled natively.
test:
	@cargo test --workspace

coverage:
	@cargo tarpaulin -p spreadsheet-core

# Desktop GUI (egui/eframe)
ext1:
	@cargo build --release -p spreadsheet --features gui
	@./target/release/spreadsheet

check: fmt clippy

fmt:
	@cargo fmt --all --check

clippy:
	@cargo clippy --workspace --all-features -- -D warnings

# ---------------------------------------------------------------- wasm / web

# Build the wasm package into web/src/wasm-pkg (needs wasm-pack + the
# wasm32-unknown-unknown target, see README).
wasm:
	@wasm-pack build crates/wasm --target web --out-dir ../../web/src/wasm-pkg

# Run the bridge tests inside wasm under Node.
wasm-test:
	@wasm-pack test --node crates/wasm

# TypeScript project: install deps, type-check + vitest bridge tests, dev server.
web-install:
	@cd web && npm install

web-test: wasm
	@cd web && npm run typecheck && npm test

# Browser end-to-end tests (one-time: cd web && npx playwright install chromium)
web-e2e: wasm
	@cd web && npm run test:e2e

web-dev: wasm
	@cd web && npm run dev

docs:
	@cargo doc --open --workspace --all-features &
	@pdflatex -interaction=batchmode report.tex

clean:
	@cargo clean
	@rm -rf web/node_modules web/dist web/src/wasm-pkg
	@rm -f report.aux report.log report.out report.pdf

.PHONY: all build test coverage ext1 check fmt clippy wasm wasm-test web-install web-test web-e2e web-dev docs clean
