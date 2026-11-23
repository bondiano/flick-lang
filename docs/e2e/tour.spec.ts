// The tour in a browser: Next leads through every lesson, and each program does what its index says.
import { readFileSync } from "node:fs";
import { expect, type Page, test } from "@playwright/test";
import { type Lesson, lessons } from "../tour/load.ts";
import { loadExercise } from "../exercises/load.ts";
import { replaceSource } from "./helpers.ts";

const all = lessons();
const shown = all.filter((entry) => entry.exercise === undefined);

for (const locale of ["", "ru/"]) {
  test(`an obsolete draft cannot break the loops lesson (${locale || "en/"})`, async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.setItem("flick.lesson.tour.loops.draft", "module Main exposing (main)\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line \"old\"\n");
    });
    await page.goto(`/flick-lang/${locale}tour/loops/`);
    await ready(page);
    await expect(page.locator(".cm-content")).toContainText("Host.Console");
    const entry = shown.find((lesson) => lesson.id === "loops")!;
    await shows(page, entry);
    await expect(page.locator(".fp-status")).not.toHaveClass(/fp-bad/);
  });
}

for (const [id, exercise] of [["guards", "guarantees/01"], ["cache", "laziness/01"], ["green", "testing/01"]] as const) {
  test(`the ${id} exercise rejects its starter and wrong output, then accepts the solution`, async ({ page }) => {
    const fixture = loadExercise(exercise);
    await page.goto(`/flick-lang/tour/${id}/`);
    await ready(page);
    await expect(page.locator(".fp-problems-panel")).toContainText(fixture.starter_error, { timeout: 30_000 });
    await page.getByRole("button", { name: "Check answer", exact: true }).click();
    await expect(page.locator(".fp-lesson")).toContainText("Fix the compiler errors first.", { timeout: 30_000 });
    await replaceSource(page, readFileSync(new URL(`../exercises/${exercise}/broken.fl`, import.meta.url), "utf8"));
    await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
    await page.getByRole("button", { name: "Check answer", exact: true }).click();
    await expect(page.locator(".fp-lesson")).toContainText("The output does not match the task.", { timeout: 30_000 });
    await page.getByRole("button", { name: "Show solution", exact: true }).click();
    await expect(page.locator(".fp-lesson")).not.toContainText("Correct. The signature and output match.");
    await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
    await page.getByRole("button", { name: "Check answer", exact: true }).click();
    await expect(page.locator(".fp-lesson")).toContainText("Correct. The signature and output match.", { timeout: 30_000 });
  });
}

/** The compiler has loaded and checked the source, which may have errors, and the editor has opened. */
const ready = async (page: Page): Promise<void> => {
  if (await page.locator(".tour-local").count()) {
    await expect(page.locator(".tour-local pre")).toBeVisible();
    return;
  }
  await expect(page.locator(".fp-status")).not.toHaveClass(/fp-status-loading/, { timeout: 60_000 });
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible();
};

/** The page shows what the lesson's program does, without a press of Run. */
const shows = async (page: Page, entry: Lesson): Promise<void> => {
  if (entry.exercise !== undefined) {
    await expect(page.locator(".fp-lesson")).toBeVisible();
  } else if (entry.expect === "local") {
    await expect(page.locator(".tour-local")).toContainText("Checked in CI");
    await expect(page.locator("#flick-playground")).toHaveCount(0);
  } else if (entry.expect === "run") {
    await expect(page.locator(".fp-output")).toHaveText(readFileSync(entry.stdout ?? "", "utf8").trim(), { timeout: 30_000 });
  } else if (entry.expect === "tests") {
    for (const text of entry.local?.contains ?? []) await expect(page.locator(".fp-output")).toContainText(text, { timeout: 30_000 });
    await expect(page.locator(".fp-exit")).toContainText(`Exit code ${entry.local?.exit ?? 0}`);
  } else if (entry.expect === "ui") {
    // as the playground's own test of an interface, give the first preview a minute
    await expect(page.frameLocator(".fp-frame iframe").locator("#app")).not.toBeEmpty({ timeout: 60_000 }).catch(async (error: unknown) => {
      throw new Error(`${entry.id} shows no interface; the result says: ${await page.locator(".fp-result").innerText()}`, { cause: error });
    });
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
  await page.keyboard.insertText('module Main exposing (main)\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line "edited"\n');
  await expect(page.locator(".fp-output")).toContainText("edited", { timeout: 30_000 });
});

test("the app's sibling children stay visible while books are finished", async ({ page }) => {
  await page.goto("/flick-lang/tour/app-ui/");
  await ready(page);
  const root = page.frameLocator(".fp-frame iframe").locator("#app");
  await expect(root.locator("ul + p")).toHaveText("pages left: 886", { timeout: 60_000 });
  await expect(root.getByRole("button", { name: "finish", exact: true })).toHaveCount(2);
  await root.getByRole("button", { name: "finish", exact: true }).first().click();
  await expect(root.locator("ul + p")).toHaveText("pages left: 474");
  await expect(root.locator("li").first()).toHaveText("[x] Dune");
  await root.getByRole("button", { name: "finish", exact: true }).click();
  await expect(root.locator("ul + p")).toHaveText("pages left: 0");
  await expect(root.getByRole("button", { name: "finish", exact: true })).toHaveCount(0);
});

test("a Russian native-only lesson shows checked code, not a runnable editor", async ({ page }) => {
  await page.goto("/flick-lang/ru/tour/companions/");
  await ready(page);
  await expect(page.locator(".tour-local")).toContainText("Локальный пример. Проверяется в CI");
  await expect(page.locator(".tour-local")).toContainText("module Main");
  await expect(page.locator("#flick-playground")).toHaveCount(0);
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
  await page.keyboard.insertText('module Main exposing (main)\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line "behind a tab"\n');
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
  await page.keyboard.insertText('module Main exposing (main)\n\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line "from a link"\n');
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

for (const locale of ["", "ru/"]) {
  test(`the whole tour is one page of text and code, without an editor (${locale || "en/"})`, async ({ page }) => {
    await page.goto(`/flick-lang/${locale}tour/all/`);
    await expect(page.locator(".tour-all-lesson")).toHaveCount(all.length);
    await expect(page.locator("#flick-playground")).toHaveCount(0);
    const open = page.locator(`.tour-all-lesson a[href="../${all[0]?.id ?? ""}/"]`);
    await open.click();
    await expect(page).toHaveURL(new RegExp(`/${locale}tour/${all[0]?.id ?? ""}/$`));
  });
}

test("a Russian lesson leads to the next Russian lesson", async ({ page }) => {
  const [first, second] = all;
  test.skip(second === undefined, "the tour has one lesson");
  await page.goto(`/flick-lang/ru/tour/${first?.id ?? ""}/`);
  await page.locator('a[rel="next"]').click();
  await expect(page).toHaveURL(new RegExp(`/ru/tour/${second?.id ?? ""}/$`));
});
