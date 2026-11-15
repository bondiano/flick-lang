// The sandboxed UI preview, in both playground locales.
import { expect, test } from "@playwright/test";

import { ready } from "./helpers.ts";

test("an interface runs in a sandbox and Run starts it again", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "A counter" }).click();
  await page.getByRole("button", { name: "Run", exact: true }).click();
  const frame = page.frameLocator(".fp-frame iframe");
  await expect(frame.getByText("clicked 0 times")).toBeVisible({ timeout: 60_000 });
  await frame.getByRole("button", { name: "+1" }).click();
  await expect(frame.getByText("clicked 1 times")).toBeVisible();
  await frame.locator("body").evaluate(() => parent.postMessage({ stream: "stdout", text: "later output" }, "*"));
  await expect(page.locator(".fp-output")).toContainText("later output");
  const isolated = await frame.locator("body").evaluate(async () => {
    let parentBlocked = false;
    try { void parent.document.title; } catch { parentBlocked = true; }
    let networkBlocked = false;
    try { await fetch("http://127.0.0.1:4322/flick-lang/"); } catch { networkBlocked = true; }
    return parentBlocked && networkBlocked;
  });
  expect(isolated).toBe(true);
  await expect(page.getByRole("button", { name: "Run", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(frame.getByText("clicked 0 times")).toBeVisible({ timeout: 60_000 });
  await expect(page.locator(".fp-frame iframe")).toHaveCount(1);
  await frame.locator("body").evaluate(() => parent.postMessage({ stream: "crash", text: "late failure" }, "*"));
  await expect(page.locator(".fp-failure-detail")).toContainText("late failure");
  await expect(page.locator(".fp-frame iframe")).toHaveCount(0);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "Hello" }).click();
  await expect(page.locator(".fp-frame iframe")).toHaveCount(0);
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText("Hello from Flick");
});

test("the Russian playground previews the counter", async ({ page }) => {
  await page.goto("/flick-lang/ru/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "Счётчик" }).click();
  await page.getByRole("button", { name: "Запустить", exact: true }).click();
  // Running a program has a ten-second deadline in Actions.run_program.
  await expect(page.frameLocator(".fp-frame iframe").getByText("clicked 0 times")).toBeVisible({ timeout: 10_000 });
  await expect(page.getByRole("status").filter({ hasText: "Работает в изолированном фрейме" })).toBeVisible();
});
