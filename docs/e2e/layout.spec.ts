import { expect, test } from "@playwright/test";

for (const locale of ["en", "ru"]) {
  test(`documentation keeps a normal gutter beside the sidebar (${locale})`, async ({ page }) => {
    for (const width of [390, 800, 1024, 1152, 1440, 1920]) {
      await page.setViewportSize({ width, height: 1000 });
      await page.goto(`/flick-lang/${locale === "ru" ? "ru/" : ""}start/why/`);
      const heading = await page.locator("h1#_top").boundingBox();
      const article = await page.locator(".sl-markdown-content").boundingBox();
      const sidebar = await page.locator(".site-docs-sidebar").boundingBox();
      expect(heading).not.toBeNull();
      expect(article).not.toBeNull();
      if (heading === null || article === null) throw new Error("missing article");
      const gutter = article.x - (sidebar === null ? 0 : sidebar.x + sidebar.width);
      expect(gutter, `${width}px: article gutter`).toBeGreaterThanOrEqual(16);
      expect(gutter, `${width}px: article gutter`).toBeLessThanOrEqual(32);
      expect(Math.abs(heading.x - article.x), `${width}px: heading alignment`).toBeLessThan(1);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    }
  });
}

for (const width of [1440, 1920, 2560]) {
  test(`the tour uses the available width at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 1200 });
    await page.goto("/flick-lang/tour/program/");
    const lesson = await page.locator(".tour-lesson").boundingBox();
    const prose = await page.locator(".tour-prose").boundingBox();
    const editor = await page.locator(".tour-editor").boundingBox();
    expect(lesson).not.toBeNull();
    expect(prose).not.toBeNull();
    expect(editor).not.toBeNull();
    if (lesson === null || prose === null || editor === null) throw new Error("missing tour layout");
    expect(lesson.x + lesson.width).toBeGreaterThan(width - 64);
    expect(prose.width).toBeLessThanOrEqual(480);
    expect(editor.x).toBeGreaterThan(prose.x + prose.width);
    expect(editor.width).toBeGreaterThan(500);
  });
}

for (const [locale, name] of [["", "Hello"], ["ru/", "Приветствие"]] as const) {
  for (const width of [390, 1440]) {
    test(`the example picker opens below its button (${locale || "en/"}, ${width}px)`, async ({ page }) => {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`/flick-lang/${locale}play/`);
      const button = page.locator(".fp-gallery-button");
      await button.click();
      const gallery = page.locator(".fp-gallery");
      await expect(gallery).toBeVisible();
      const trigger = await button.boundingBox();
      const panel = await gallery.boundingBox();
      if (trigger === null || panel === null) throw new Error("missing example picker");
      expect(panel.y).toBeGreaterThanOrEqual(trigger.y + trigger.height);
      expect(panel.y - trigger.y - trigger.height).toBeLessThanOrEqual(12);
      expect(Math.abs(panel.x - trigger.x)).toBeLessThanOrEqual(8);
      expect(panel.x + panel.width).toBeLessThanOrEqual(width);
      expect(panel.y + panel.height).toBeLessThanOrEqual(900);
      await page.keyboard.press("Escape");
      await expect(gallery).toBeHidden();
      await expect(button).toBeFocused();
      await page.keyboard.press("Enter");
      await expect(gallery).toBeVisible();
      await gallery.getByRole("button", { name, exact: true }).click();
      await expect(gallery).toBeHidden();
      await expect(button).toContainText(name);
      await button.click();
      await page.mouse.click(width - 2, 65);
      await expect(gallery).toBeHidden();
    });
  }
}

test("a narrow tour stacks its text and editor without horizontal overflow", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/flick-lang/ru/tour/program/");
  const prose = await page.locator(".tour-prose").boundingBox();
  const editor = await page.locator(".tour-editor").boundingBox();
  if (prose === null || editor === null) throw new Error("missing tour layout");
  expect(editor.y).toBeGreaterThanOrEqual(prose.y + prose.height);
  expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
});
