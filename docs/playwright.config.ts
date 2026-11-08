import { defineConfig } from "@playwright/test";

// The built site, served the way GitHub Pages serves it, in a real browser. `CHROMIUM` names one of the
// system, as for Conduit's tests; without it Playwright uses the browser `playwright install` fetched.
export default defineConfig({
  testDir: "e2e",
  timeout: 90_000,
  forbidOnly: process.env.CI !== undefined,
  use: {
    baseURL: "http://127.0.0.1:4322",
    launchOptions: process.env.CHROMIUM === undefined ? {} : { executablePath: process.env.CHROMIUM },
  },
  webServer: {
    command: "pnpm exec astro preview --host 127.0.0.1 --port 4322",
    url: "http://127.0.0.1:4322/flick-lang/",
    reuseExistingServer: process.env.CI === undefined,
  },
});
