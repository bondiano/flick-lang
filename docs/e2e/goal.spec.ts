// The goal beside an exercise's editor: the export and output its check compares, on every page of it.
import { expect, type Page, test } from "@playwright/test";
import { replaceSource } from "./helpers.ts";

const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-status")).not.toHaveClass(/fp-status-loading/, { timeout: 60_000 });
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible();
};

for (const [locale, words] of [
  ["", { goal: "Goal", export: "Export (.d.ts)", output: "Expected output" }],
  ["ru/", { goal: "Цель", export: "Экспорт (.d.ts)", output: "Ожидаемый вывод" }],
] as const) {
  test(`the goal follows the exercise through Next, Back, a draft and a reload (${locale || "en/"})`, async ({ page }) => {
    const goal = page.getByRole("group", { name: words.goal, exact: true });
    const shows = async (signature: string, output: string): Promise<void> => {
      await expect(goal.getByRole("heading", { level: 3 })).toHaveText(words.goal);
      await expect(goal.locator(".fp-goal-row").nth(0)).toHaveText(`${words.export}${signature}`);
      await expect(goal.locator(".fp-goal-row").nth(1)).toHaveText(`${words.output}${output}`);
    };
    await page.goto(`/flick-lang/${locale}course/values-01/`);
    await ready(page);
    await shows("half: (number) => number", "3.5");
    await page.locator(".cm-content").click();
    await page.keyboard.press("ControlOrMeta+End");
    await page.keyboard.type("\n// my draft\n");
    await page.locator('a[rel="next"]').click();
    await expect(page).toHaveURL(/\/course\/values-02\/$/);
    await ready(page);
    await shows("area: (number) => (number) => number", "12");
    await expect(page.locator(".cm-content")).not.toContainText("my draft");
    await page.goBack();
    await expect(page).toHaveURL(/\/course\/values-01\/$/);
    await ready(page);
    await shows("half: (number) => number", "3.5");
    await expect(page.locator(".cm-content")).toContainText("my draft");
    await page.reload();
    await ready(page);
    await shows("half: (number) => number", "3.5");
    await expect(page.locator(".cm-content")).toContainText("my draft");
  });
}

test("a running export's goal is an Effect, and a page without an exercise has no goal", async ({ page }) => {
  await page.goto("/flick-lang/tour/failures/");
  await ready(page);
  await expect(page.locator(".fp-goal .fp-goal-row").nth(0)).toHaveText("Export (.d.ts)total_age: (string) => Effect<number, never, never>");
  await page.goto("/flick-lang/tour/program/");
  await ready(page);
  await expect(page.locator(".fp-goal")).toHaveCount(0);
  await page.goto("/flick-lang/play/");
  await ready(page);
  await expect(page.locator(".fp-goal")).toHaveCount(0);
});

test("a todo in a signature is T024, and neither its error nor the solution completes the exercise", async ({ page }) => {
  await page.goto("/flick-lang/ru/course/values-03/");
  await ready(page);
  const goal = page.locator(".fp-goal");
  await expect(goal).toContainText("todo в сигнатуре — ошибка T024", { timeout: 30_000 });
  await page.getByRole("button", { name: "Int -> Int -> Float", exact: true }).click();
  await page.getByRole("button", { name: "Проверить", exact: true }).click();
  await expect(page.locator(".fp-lesson")).toContainText("Сначала исправьте ошибки компилятора.", { timeout: 30_000 });
  await page.getByRole("button", { name: "Показать решение", exact: true }).click();
  await expect(goal).not.toContainText("T024", { timeout: 30_000 });
  await expect(page.locator(".fp-lesson")).not.toContainText("Урок пройден");
  expect(await page.evaluate(() => localStorage.getItem("flick.lesson.values.03.done"))).toBeNull();
});

test("a todo in an expression type-checks with T022 but fails the check when it runs", async ({ page }) => {
  await page.goto("/flick-lang/course/values-01/");
  await ready(page);
  const goal = page.locator(".fp-goal");
  await expect(goal).not.toContainText("T022");
  await replaceSource(page, "module Main exposing (main, half)\n\nhalf : Float -> Float\nlet half x = todo\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (half 7.0))\n");
  await expect(goal).toContainText("A todo in an expression takes the type its place needs", { timeout: 30_000 });
  await expect(page.locator(".fp-status")).toContainText("Type-checked");
  await page.getByRole("button", { name: "Check answer", exact: true }).click();
  await expect(page.locator(".fp-lesson")).toContainText("The output does not match the task.", { timeout: 30_000 });
  await expect(page.locator(".fp-lesson")).not.toContainText("Lesson completed");
  expect(await page.evaluate(() => localStorage.getItem("flick.lesson.values.01.done"))).toBeNull();
});

/** WCAG's contrast ratio of two CSS colors, through a canvas, which reads any color syntax as sRGB. */
const contrast = (page: Page, foreground: string, background: string): Promise<number> =>
  page.evaluate(([fg, bg]) => {
    const context = document.createElement("canvas").getContext("2d", { willReadFrequently: true });
    if (context === null) throw new Error("no canvas");
    const luminance = (color: string): number => {
      context.fillStyle = color;
      context.fillRect(0, 0, 1, 1);
      const [r = 0, g = 0, b = 0] = context.getImageData(0, 0, 1, 1).data;
      const channel = (value: number): number => (value / 255 <= 0.04045 ? value / 255 / 12.92 : ((value / 255 + 0.055) / 1.055) ** 2.4);
      return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
    };
    const [high, low] = [luminance(fg), luminance(bg)].sort((a, b) => b - a);
    return ((high ?? 0) + 0.05) / ((low ?? 0) + 0.05);
  }, [foreground, background]);

for (const colorScheme of ["light", "dark"] as const) {
  for (const width of [390, 768, 1440]) {
    test(`the goal is readable and leaves the editor's actions free at ${width}px (${colorScheme})`, async ({ page }) => {
      await page.emulateMedia({ colorScheme });
      await page.setViewportSize({ width, height: 900 });
      // a signature todo adds its note to the goal once the compiler checks the starter
      await page.goto("/flick-lang/course/values-03/");
      await ready(page);
      const goal = page.locator(".fp-goal");
      await expect(goal.locator("p")).toBeVisible({ timeout: 30_000 });
      const box = await goal.boundingBox();
      if (box === null) throw new Error("missing goal");
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(width);
      expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(width);
      const background = await goal.evaluate((element) => getComputedStyle(element).backgroundColor);
      for (const part of [".fp-goal-label", ".fp-goal-value", "p"]) {
        const color = await goal.locator(part).first().evaluate((element) => getComputedStyle(element).color);
        expect(await contrast(page, color, background), `${part} contrast`).toBeGreaterThanOrEqual(4.5);
      }
      // each action of the editor is the element at its own center: nothing lies over it
      for (const name of ["Check answer", "Show solution", "Run", "Format"]) {
        const button = page.getByRole("button", { name, exact: true });
        await button.scrollIntoViewIfNeeded();
        expect(await button.evaluate((element) => {
          const { x, y, width, height } = element.getBoundingClientRect();
          return element.contains(document.elementFromPoint(x + width / 2, y + height / 2));
        }), `${name} is uncovered`).toBe(true);
      }
    });
  }
}
