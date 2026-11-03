// The RealWorld Playwright suite, vendored unchanged, against the client
// as the Go server serves it (`flick run e2e` starts both and a fresh
// database). `fullstack`: the tests drive everything through the pages
// and make their own users.
import { defineConfig } from "@playwright/test";

import { baseConfig } from "../vendor/realworld/specs/e2e/playwright.base";

process.env.TEST_MODE ??= "fullstack";

export default defineConfig({
  ...baseConfig,
  testDir: "../vendor/realworld/specs/e2e",
  reporter: "list",
  use: {
    ...baseConfig.use,
    baseURL: process.env.BASE_URL ?? "http://localhost:8000",
    launchOptions: process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {},
  },
});
