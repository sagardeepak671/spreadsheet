/// <reference types="vitest/config" />
import { defineConfig } from "vite";

export default defineConfig({
  base: "./",
  // The wasm-pack output (web/src/wasm-pkg) is plain ES modules + a .wasm
  // file loaded with `new URL(..., import.meta.url)`, which Vite handles
  // natively; no wasm plugin needed.
  build: {
    target: "es2022",
  },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
