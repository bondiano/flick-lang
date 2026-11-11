// The course in a browser: hints one at a time, a question, Ctrl+Enter and the progress of the contents.
import { expect, type Page, test } from "@playwright/test";
import { courses, pageOf } from "../course/load.ts";

const exercises = courses().flatMap((course) => course.lessons.flatMap((lesson) => lesson.exercises));
const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-status")).not.toHaveClass(/fp-status-loading/, { timeout: 60_000 });
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible();
};

test("Next leads through every exercise of the course", async ({ page }) => {
  test.setTimeout(exercises.length * 20_000);
  await page.goto("/flick-lang/course/");
  await page.locator(".tour-chapters a").first().click();
  for (const [index, id] of exercises.entries()) {
    await expect(page).toHaveURL(new RegExp(`/course/${pageOf(id)}/$`));
    await expect(page.locator(".fp-lesson")).toBeVisible({ timeout: 60_000 });
    if (index < exercises.length - 1) await page.locator('a[rel="next"]').click();
  }
});

test("hints open one at a time, and the last lesson has none", async ({ page }) => {
  await page.goto("/flick-lang/course/values-01/");
  await ready(page);
  const lesson = page.locator(".fp-lesson");
  await page.getByRole("button", { name: "Hint", exact: true }).click();
  await expect(lesson.locator("li")).toHaveCount(1);
  await page.getByRole("button", { name: "Next hint", exact: true }).click();
  await expect(lesson.locator("li")).toHaveCount(2);
  await expect(page.getByRole("button", { name: "Next hint", exact: true })).toBeDisabled();
  await page.goto("/flick-lang/course/capstone-01/");
  await ready(page);
  await expect(page.getByRole("button", { name: "Hint", exact: true })).toHaveCount(0);
});

test("a question is answered before the code counts, and Ctrl+Enter checks the answer", async ({ page }) => {
  await page.goto("/flick-lang/course/values-03/");
  await ready(page);
  const lesson = page.locator(".fp-lesson");
  await page.getByRole("button", { name: "Show solution" }).click();
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(lesson).toContainText("Answer the question first.", { timeout: 30_000 });
  await page.getByRole("button", { name: "Int -> Int -> Int", exact: true }).click();
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(lesson).toContainText("That answer is not right.", { timeout: 30_000 });
  await page.getByRole("button", { name: "Int -> Int -> Float", exact: true }).click();
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+Enter");
  await expect(lesson).toContainText("Correct.", { timeout: 30_000 });
  await page.goto("/flick-lang/course/");
  await expect(page.locator(".course-progress")).toContainText(`Exercises done: 1 of ${exercises.length}.`, { timeout: 30_000 });
  await expect(page.locator(".course-progress a")).toHaveAttribute("href", `./${pageOf(exercises[0] ?? "")}/`);
});

test("the Russian contents count the same exercises", async ({ page }) => {
  await page.goto("/flick-lang/ru/course/");
  await expect(page.locator(".course-progress")).toContainText(`Пройдено упражнений: 0 из ${exercises.length}.`, { timeout: 30_000 });
});
