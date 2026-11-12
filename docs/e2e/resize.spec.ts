import { expect, type Page, test } from "@playwright/test";

// Exercises intentionally open with diagnostics, so readiness cannot require "Type-checked".
const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible({ timeout: 60_000 });
  await expect(page.locator(".fp-status")).not.toHaveClass(/fp-status-loading/, { timeout: 60_000 });
};

for (const route of ["play", "tour/program", "course/values-01"]) {
  test(`${route}: panels resize by dragging and keys, survive edits and reload`, async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 1000 });
    await page.goto(`/flick-lang/${route}/`);
    await ready(page);
    const split = page.getByRole("separator");
    await expect(split).toBeVisible();
    await split.scrollIntoViewIfNeeded();
    const stacked = route !== "play";
    await expect(split).toHaveAttribute("aria-orientation", stacked ? "horizontal" : "vertical");
    const source = page.locator(".fp-source");
    const before = await source.boundingBox();
    const box = await split.boundingBox();
    if (before === null || box === null) throw new Error("missing panels");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + (stacked ? 0 : 100), box.y + box.height / 2 + (stacked ? 80 : 0), { steps: 5 });
    await page.mouse.up();
    const after = await source.boundingBox();
    expect(stacked ? after?.height : after?.width).toBeGreaterThan((stacked ? before.height : before.width) + 40);
    await split.focus();
    await page.keyboard.press("Home");
    await expect(split).toHaveAttribute("aria-valuenow", "20");
    await page.keyboard.press(stacked ? "ArrowDown" : "ArrowRight");
    await expect(split).toHaveAttribute("aria-valuenow", "25");
    await page.locator(".cm-content").click();
    await page.keyboard.press("ControlOrMeta+End");
    await page.keyboard.insertText("\n// keep the panel size\n");
    await expect(split).toHaveAttribute("aria-valuenow", "25");
    await page.reload();
    await ready(page);
    await expect(split).toHaveAttribute("aria-valuenow", "25");
    await split.focus();
    await page.keyboard.press("End");
    await expect(split).toHaveAttribute("aria-valuenow", "80");
  });
}

test("lesson splitters follow the panel orientation and disappear on phones", async ({ page }) => {
  await page.setViewportSize({ width: 2200, height: 1000 });
  await page.goto("/flick-lang/tour/program/");
  await ready(page);
  const split = page.getByRole("separator");
  await expect(split).toBeVisible();
  await expect(split).toHaveAttribute("aria-orientation", "vertical");
  await page.setViewportSize({ width: 1280, height: 1000 });
  await expect(split).toHaveAttribute("aria-orientation", "horizontal");
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(split).toBeHidden();
  const panels = await page.locator(".fp-panes").boundingBox();
  expect(panels?.width).toBeLessThanOrEqual(390);
});
