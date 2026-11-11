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
