// The playground in a browser, with the real WebAssembly compiler.
import { expect, type Page, test } from "@playwright/test";

/** The compiler has loaded and checked the first example, and the editor has opened. */
const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-status")).toContainText("0.1.0", { timeout: 60_000 });
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible();
};

/** Moves the pointer onto the first `word` in the editor's line that holds `line`. */
const hoverWord = async (page: Page, line: string, word: string): Promise<void> => {
  const box = await page.locator(".cm-line", { hasText: line }).first().evaluate((element, word) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
      const at = node.textContent?.indexOf(word) ?? -1;
      if (at < 0) continue;
      const range = document.createRange();
      range.setStart(node, at);
      range.setEnd(node, at + word.length);
      const { x, y, width, height } = range.getBoundingClientRect();
      return { x: x + width / 2, y: y + height / 2 };
    }
    throw new Error(`no ${word} in the line`);
  }, word);
  await page.mouse.move(box.x, box.y);
};

test("Run prints what the program writes", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText("Hello from Flick", { timeout: 60_000 });
});

test("an inline module compiles and runs in the browser", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+A");
  await page.keyboard.insertText("module Main exposing (main)\n\nmodule Child exposing (answer) =\n  let answer = 42\n\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show Child.answer)\n");
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText("42", { timeout: 60_000 });
});

test("a shared program opens without running and a draft survives reload", async ({ page, context }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("\n// shared draft\n");
  await page.getByRole("button", { name: "Share" }).click();
  await expect(page).toHaveURL(/#code=/);
  const linked = await context.newPage();
  await linked.goto(page.url());
  await ready(linked);
  await expect(linked.locator(".cm-content")).toContainText("shared draft");
  await expect(linked.getByRole("button", { name: "Run", exact: true })).toBeVisible();
  await expect(linked.locator(".fp-output, .fp-frame-open")).toHaveCount(0);
  await page.reload();
  await ready(page);
  await expect(page.locator(".cm-content")).toContainText("shared draft");
  await page.getByRole("button", { name: "Clear draft" }).click();
  await page.goto("/flick-lang/play/");
  await ready(page);
  await expect(page.locator(".cm-content")).not.toContainText("shared draft");
});

test("source downloads as a Flick file", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Download .fl" }).click();
  expect((await download).suggestedFilename()).toBe("Main.fl");
});

test("editing works when browser storage is blocked", async ({ page, context }) => {
  await context.addInitScript(() => {
    Object.defineProperty(window, "localStorage", { configurable: true, get: () => { throw new Error("blocked"); } });
  });
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("\n// storage blocked\n");
  await expect(page.locator(".cm-content")).toContainText("storage blocked");
});

test("a signature lesson checks a correction and remembers completion", async ({ page }) => {
  await page.goto("/flick-lang/tour/signatures/");
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible({ timeout: 60_000 });
  await page.getByRole("button", { name: "Check answer" }).click();
  await expect(page.locator(".fp-lesson")).toContainText("Fix the compiler errors first");
  await page.getByRole("button", { name: "Hint" }).click();
  await expect(page.locator(".fp-lesson")).toContainText("String.length returns Int");
  await page.getByRole("button", { name: "Show solution" }).click();
  await page.getByRole("button", { name: "Check answer" }).click();
  await expect(page.locator(".fp-lesson")).toContainText("Correct. The signature says String -> Int", { timeout: 30_000 });
  await page.reload();
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible({ timeout: 60_000 });
  await expect(page.locator(".fp-lesson")).toContainText("Lesson completed");
});

test("an interface runs in a sandbox and Run starts it again", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "A counter" }).click();
  await page.getByRole("button", { name: "Run", exact: true }).click();
  const frame = page.frameLocator(".fp-frame iframe");
  await expect(frame.getByText("clicked 0 times")).toBeVisible({ timeout: 60_000 });
  await frame.getByRole("button", { name: "+1" }).click();
  await expect(frame.getByText("clicked 1 times")).toBeVisible();
  await frame.locator("body").evaluate(() => parent.postMessage({ stream: "stdout", text: "later output" }, "*"));
  await expect(page.locator(".fp-output")).toContainText("later output");
  const isolated = await frame.locator("body").evaluate(async () => {
    let parentBlocked = false;
    try { void parent.document.title; } catch { parentBlocked = true; }
    let networkBlocked = false;
    try { await fetch("http://127.0.0.1:4322/flick-lang/"); } catch { networkBlocked = true; }
    return parentBlocked && networkBlocked;
  });
  expect(isolated).toBe(true);
  await expect(page.getByRole("button", { name: "Run", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(frame.getByText("clicked 0 times")).toBeVisible({ timeout: 60_000 });
  await expect(page.locator(".fp-frame iframe")).toHaveCount(1);
  await frame.locator("body").evaluate(() => parent.postMessage({ stream: "crash", text: "late failure" }, "*"));
  await expect(page.locator(".fp-failure-detail")).toContainText("late failure");
  await expect(page.locator(".fp-frame iframe")).toHaveCount(0);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "Hello" }).click();
  await expect(page.locator(".fp-frame iframe")).toHaveCount(0);
  await page.getByRole("button", { name: "Run", exact: true }).click();
  await expect(page.locator(".fp-output")).toContainText("Hello from Flick");
});

test("the Russian playground previews the counter", async ({ page }) => {
  await page.goto("/flick-lang/ru/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "Счётчик" }).click();
  await page.getByRole("button", { name: "Запустить", exact: true }).click();
  await expect(page.frameLocator(".fp-frame iframe").getByText("clicked 0 times")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Работает в изолированном фрейме" })).toBeVisible();
});

test("the old Transpile page opens the JavaScript tab", async ({ page }) => {
  await page.goto("/flick-lang/transpile/");
  await expect(page).toHaveURL(/\/play\/\?tab=js$/);
  await ready(page);
  await expect(page.locator(".fp-tabs .fp-on")).toHaveText("JavaScript");
  await expect(page.locator(".fp-generated-code")).toContainText("@flick-lang/effect-ts");
});

test("an example from the gallery fails with its code", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "A type error" }).click();
  await expect(page.locator(".fp-gallery")).toBeHidden();
  await expect(page.locator(".fp-status")).toContainText("Errors", { timeout: 60_000 });
  await expect(page.locator(".fp-problems-panel")).toContainText("FLICK-T003");
  await page.getByRole("link", { name: "Full explanation" }).first().click();
  await expect(page).toHaveURL(/\/reference\/errors\/flick-t003\/$/);
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

test("hovering a name shows its type", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await hoverWord(page, "let main", "main");
  await expect(page.locator(".fp-hover")).toContainText("Unit", { timeout: 30_000 });
});

test("an edit is checked, underlined and explained", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type('\n\nwrong : Int\nlet wrong = "text"\n');
  await expect(page.locator(".fp-status")).toContainText("Errors", { timeout: 30_000 });
  await expect(page.locator(".cm-lintRange-error").first()).toBeVisible();
  await page.locator(".fp-explain").first().click();
  await expect(page.locator(".fp-explanation")).not.toBeEmpty();
});

test("completion offers what a module exposes", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("\n\nlet xs = List.");
  await expect(page.locator(".cm-tooltip-autocomplete")).toContainText("map", { timeout: 30_000 });
});

test("Format rewrites the source as flick fmt does", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.type("\n\nlet   spaced   =   1\n");
  await page.getByRole("button", { name: "Format", exact: true }).click();
  await expect(page.locator(".cm-content")).toContainText("let spaced = 1");
});

test("a quick fix adds the forgotten case", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "A forgotten case" }).click();
  await expect(page.locator(".fp-status")).toContainText("Errors", { timeout: 30_000 });
  await page.locator(".cm-lintRange-error").first().hover();
  await page.locator(".cm-diagnosticAction").click();
  await expect(page.locator(".cm-content")).toContainText("| Paused ->");
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
});
