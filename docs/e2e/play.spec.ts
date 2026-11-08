// The playground in a browser, with the real WebAssembly compiler.
import { expect, type Page, test } from "@playwright/test";

/** The compiler has loaded and checked the first example. */
const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-status")).toContainText("0.1.0", { timeout: 60_000 });
};

test("Run prints what the program writes", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText("Hello from Flick", { timeout: 60_000 });
});

test("the old Transpile page opens the JavaScript tab", async ({ page }) => {
  await page.goto("/flick-lang/transpile/");
  await expect(page).toHaveURL(/\/play\/\?tab=js$/);
  await ready(page);
  await expect(page.locator(".fp-tabs .fp-on")).toHaveText("JavaScript");
  await expect(page.locator(".fp-generated-code")).toContainText("@flick-lang/runtime");
});

test("an example from the gallery fails with its code", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "A type error" }).click();
  await expect(page.locator(".fp-gallery")).toBeHidden();
  await expect(page.locator(".fp-status")).toContainText("Errors", { timeout: 60_000 });
  await expect(page.locator(".fp-problems-panel")).toContainText("FLICK-T003");
});

test("the splitter moves with the keys and stays where it was put", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  const split = page.getByRole("separator");
  await split.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(split).toHaveAttribute("aria-valuenow", "50");
  await page.reload();
  await expect(page.getByRole("separator")).toHaveAttribute("aria-valuenow", "50");
});
