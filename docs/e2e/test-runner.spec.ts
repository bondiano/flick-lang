// The playground runs a module's native suites in its program worker: the report, listing, filtering, repeated runs and Stop.
import { expect, type Page, test } from "@playwright/test";
import { replaceSource } from "./helpers.ts";

const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-status")).not.toHaveClass(/fp-status-loading/, { timeout: 60_000 });
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible();
};
const button = (page: Page, name: string) => page.getByRole("button", { name, exact: true });

test("List tests names the selected cases without running bodies or hooks", async ({ page }) => {
  await page.goto("/flick-lang/tour/hooks/");
  await ready(page);
  await expect(page.locator(".fp-output")).toContainText("hook: after_all", { timeout: 30_000 });
  await button(page, "List tests").click();
  await button(page, "Run").click();
  await expect(page.locator(".fp-output")).toHaveText("Main > lifecycle > ordinary\nMain > lifecycle > timeout (intentionally red)", { timeout: 30_000 });
  await expect(page.locator(".fp-exit")).toContainText("Exit code 0");
});

test("the filter selects cases by full name, and repeated runs use fresh workers", async ({ page }) => {
  await page.goto("/flick-lang/tour/each/");
  await ready(page);
  await expect(page.locator(".fp-output")).toContainText("3 passed", { timeout: 30_000 });
  await page.getByRole("textbox", { name: "Test filter" }).fill("Grace");
  for (let run = 0; run < 3; run++) {
    await button(page, "Run").click();
    await expect(page.locator(".fp-output")).toContainText("1 passed · 0 failed · 1 total", { timeout: 30_000 });
    await expect(page.locator(".fp-output")).not.toContainText("Ada");
  }
});

test("a module without suites says so in Tests mode, and Main still runs main", async ({ page }) => {
  await page.goto("/flick-lang/tour/each/");
  await ready(page);
  await replaceSource(page, 'module Main exposing (main)\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line "only main"\n');
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
  await button(page, "Run").click();
  await expect(page.locator(".fp-exit")).toContainText("No test suites in this module.", { timeout: 30_000 });
  await button(page, "Main").click();
  await button(page, "Run").click();
  await expect(page.locator(".fp-output")).toHaveText("only main", { timeout: 30_000 });
});

test("Stop interrupts a suite that never ends, and the next run starts clean", async ({ page }) => {
  await page.goto("/flick-lang/tour/hooks/");
  await ready(page);
  await expect(page.locator(".fp-output")).toContainText("hook: after_all", { timeout: 30_000 });
  await replaceSource(page, 'module Main\n\nsuite "slow" {\n  test "forever" timeout 60000 { Effect.never () }\n}\n');
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
  await expect(button(page, "Stop")).toBeVisible({ timeout: 30_000 });
  await button(page, "Stop").click();
  await expect(page.locator(".fp-exit")).toContainText("Stopped", { timeout: 5_000 });
  await replaceSource(page, 'module Main\n\nimport Test exposing (expect, to)\n\nsuite "fast" {\n  test "sum" { expect (1 + 1) |> to.equal 2 }\n}\n');
  await expect(page.locator(".fp-output")).toContainText("1 passed · 0 failed · 1 total", { timeout: 30_000 });
});

test("the Russian testing lessons run the same report", async ({ page }) => {
  await page.goto("/flick-lang/ru/tour/fixtures/");
  await ready(page);
  await expect(page.locator(".fp-output")).toContainText("setup: fixture connection: expected 1, got 2", { timeout: 30_000 });
  await expect(page.getByRole("button", { name: "Список тестов", exact: true })).toBeVisible();
});
