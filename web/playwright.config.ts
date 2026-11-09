import { defineConfig, devices } from "@playwright/test";

/**
 * End-to-end tests: drive the real page (Vite dev server + wasm) in Chromium.
 * Run with `npm run test:e2e` after `npm run build:wasm` and a one-time
 * `npx playwright install chromium`.
 */
export default defineConfig({
  testDir: "./e2e",
  // perf.spec.ts measures frame times and only makes sense alone: `npm run test:perf`
  testIgnore: process.env.PERF ? [] : ["**/perf.spec.ts"],
  timeout: 60_000,
  fullyParallel: true,
  reporter: "list",
  use: {
    baseURL: "http://localhost:5174",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1200, height: 800 } } }],
  webServer: {
    command: "npx vite --port 5174 --strictPort",
    url: "http://localhost:5174",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
