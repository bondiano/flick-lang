import { expect, test } from "@playwright/test";
import { hoverWord, wordAt } from "./helpers.ts";

// The compiler loads on the first hover, so the first answer may wait for it.
const LOADING = { timeout: 60_000 };

for (const locale of ["", "ru/"]) {
  test(`a Flick panel of the landing page shows what the compiler knows (${locale || "en/"})`, async ({ page }) => {
    await page.goto(`/flick-lang/${locale}`);
    const tip = page.locator("#flick-code-hover");
    await expect(page.locator(".hover-hint")).toBeVisible();
    await page.locator(".read-code").scrollIntoViewIfNeeded();
    await hoverWord(page, "let load_view id", "load_view", ".read-code .line");
    await expect(tip).toContainText("load_view : UserId ->{Users, Log; NotFound} UserView", LOADING);
    await page.keyboard.press("Escape");
    await expect(tip).toBeHidden();

    // a name inside a plugin's block: the block's own documentation of the field
    await page.locator(".code-panel[data-sample=plugins]").scrollIntoViewIfNeeded();
    await hoverWord(page, "flag loud", "loud", ".code-panel[data-sample=plugins] .line");
    await expect(tip).toContainText("Print in capitals");
    await page.mouse.move(1, 1);
    await expect(tip).toBeHidden();
  });
}

test.describe("on a phone", () => {
  test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

  test("the landing page fits, and a tap asks the compiler", async ({ page }) => {
    await page.goto("/flick-lang/");
    expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
    await page.locator(".read-code").scrollIntoViewIfNeeded();
    const word = await wordAt(page, "let load_view id", "load_view", ".read-code .line");
    await page.touchscreen.tap(word.x, word.y);
    const tip = page.locator("#flick-code-hover");
    await expect(tip).toContainText("load_view : UserId", LOADING);
    await page.locator("#read-title").tap();
    await expect(tip).toBeHidden();
  });
});
