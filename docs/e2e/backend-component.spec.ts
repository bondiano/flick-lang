// Verifies lazy loading and SHA-256 tamper refusal of the official JS backend component.
import { expect, test } from "@playwright/test";

import { ready } from "./helpers.ts";

test("JS backend component loads lazily on emit and refuses tampered bytes without fallback", async ({ page }) => {
  const componentRequests: Array<string> = [];
  page.on("request", (req) => {
    if (req.url().includes("component") && req.url().endsWith(".wasm")) {
      componentRequests.push(req.url());
    }
  });
  await page.goto("/flick-lang/play/");
  await ready(page);
  // Checking alone never loads the backend component.
  expect(componentRequests).toEqual([]);
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText("Hello from Flick", { timeout: 60_000 });
  expect(componentRequests.length).toBeGreaterThanOrEqual(1);

  // Tampering with component.wasm bytes in a fresh session must refuse with SHA-256 mismatch (P013).
  await page.route(/component.*\.wasm$/, async (route) => {
    const response = await route.fetch();
    const body = Buffer.from(await response.body());
    body[body.length - 1] = (body[body.length - 1]! ^ 0xff) & 0xff;
    await route.fulfill({ response, body });
  });
  await page.reload();
  await ready(page);
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-exit")).toContainText("Build failed", { timeout: 60_000 });
  await expect(page.locator(".fp-diagnostics")).toContainText("FLICK-P013", { timeout: 60_000 });
  await expect(page.locator(".fp-diagnostics")).toContainText("SHA-256 mismatch", { timeout: 60_000 });
});
