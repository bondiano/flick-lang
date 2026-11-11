// The tour in a browser: Next leads through every lesson, and each program does what its index says.
import { readFileSync } from "node:fs";
import { expect, type Page, test } from "@playwright/test";
import { type Lesson, lessons } from "../tour/load.ts";

const all = lessons();
const shown = all.filter((entry) => entry.exercise === undefined);
/** The compiler has loaded and checked the source, which may have errors, and the editor has opened. */
const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-status")).not.toHaveClass(/fp-status-loading/, { timeout: 60_000 });
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible();
};
const firstLine = (file: string): string => readFileSync(file, "utf8").split("\n").find((line) => line.trim() !== "") ?? "";

/** The page shows what the lesson's program does, without a press of Run. */
const shows = async (page: Page, entry: Lesson): Promise<void> => {
  if (entry.exercise !== undefined) {
    await expect(page.locator(".fp-lesson")).toBeVisible();
  } else if (entry.expect === "run") {
    await expect(page.locator(".fp-output")).toContainText(firstLine(entry.stdout ?? ""), { timeout: 30_000 });
  } else if (entry.expect === "ui") {
    await expect(page.locator(".fp-frame-open iframe")).toBeVisible({ timeout: 30_000 });
  } else {
    await expect(page.locator(".fp-problems-panel")).toContainText(entry.expect.slice("fail ".length), { timeout: 30_000 });
  }
};

test("Next leads from the contents through every lesson, and each runs as it opens", async ({ page }) => {
  test.setTimeout(all.length * 30_000);
  await page.goto("/flick-lang/tour/");
  await page.locator(".tour-chapters a").first().click();
  for (const [index, entry] of all.entries()) {
    await expect(page).toHaveURL(new RegExp(`/tour/${entry.id}/$`));
    await ready(page);
    await shows(page, entry);
    if (index < all.length - 1) await page.locator('a[rel="next"]').click();
  }
});

test("an edit runs again once typing pauses", async ({ page }) => {
  const entry = shown.find((lesson) => lesson.expect === "run");
  test.skip(entry === undefined, "no lesson runs");
  await page.goto(`/flick-lang/tour/${entry?.id ?? ""}/`);
  await ready(page);
  await shows(page, entry as Lesson);
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText('module Main exposing (main)\n\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line "edited"\n');
  await expect(page.locator(".fp-output")).toContainText("edited", { timeout: 30_000 });
});

test("an edit made on the JavaScript tab runs on the way back to the output", async ({ page }) => {
  const entry = shown.find((lesson) => lesson.expect === "run");
  test.skip(entry === undefined, "no lesson runs");
  await page.goto(`/flick-lang/tour/${entry?.id ?? ""}/`);
  await ready(page);
  await shows(page, entry as Lesson);
  await page.getByRole("button", { name: "JavaScript", exact: true }).click();
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText('module Main exposing (main)\n\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line "behind a tab"\n');
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
  await page.waitForTimeout(1_000);
  await page.getByRole("button", { name: "Output", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText("behind a tab", { timeout: 30_000 });
});

test("code from a link waits for Run on a page of the tour", async ({ page }) => {
  const entry = shown.find((lesson) => lesson.expect === "run");
  test.skip(entry === undefined, "no lesson runs");
  await page.goto(`/flick-lang/tour/${entry?.id ?? ""}/`);
  await ready(page);
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText('module Main exposing (main)\n\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line "from a link"\n');
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.getByRole("button", { name: "Share", exact: true }).click();
  await expect(page).toHaveURL(/#code=/);
  const link = page.url();
  const other = await page.context().newPage();
  await other.goto(link);
  await ready(other);
  await expect(other.locator(".cm-content")).toContainText("from a link");
  await other.waitForTimeout(2_000);
  await expect(other.locator(".fp-output", { hasText: "from a link" })).toHaveCount(0);
  await other.getByRole("button", { name: "Run", exact: true }).click();
  await expect(other.locator(".fp-output")).toContainText("from a link", { timeout: 30_000 });
});

test("a Russian lesson leads to the next Russian lesson", async ({ page }) => {
  const [first, second] = all;
  test.skip(second === undefined, "the tour has one lesson");
  await page.goto(`/flick-lang/ru/tour/${first?.id ?? ""}/`);
  await page.locator('a[rel="next"]').click();
  await expect(page).toHaveURL(new RegExp(`/ru/tour/${second?.id ?? ""}/$`));
});
