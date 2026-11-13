import { expect, test } from "@playwright/test";

for (const locale of ["", "ru/"]) {
  test(`${locale || "en/"} package API is generated, linked and source-backed`, async ({ page }) => {
    await page.goto(`/flick-lang/${locale}reference/stdlib/`);
    await page.locator("main a").filter({ hasText: locale ? "Сгенерированный API пакета" : "Generated package API" }).click();
    await expect(page).toHaveURL(new RegExp(`/${locale}api/$`));
    await page.locator('main a[href="./string/"]').click();
    await expect(page).toHaveURL(new RegExp(`/${locale}api/string/$`));
    await expect(page.locator("main h1")).toHaveText("String");
    await expect(page.locator("main")).toContainText("length :");
    const source = page.locator('main a[href*="/packages/core/src/String.fl#L"]').first();
    await expect(source).toHaveAttribute("href", /github\.com\/bondiano\/flick-lang\/blob\/[a-f0-9]{40}\/packages\/core\/src\/String\.fl#L\d+/);
  });
}
