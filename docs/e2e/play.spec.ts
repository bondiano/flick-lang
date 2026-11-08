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

test("the old Transpile page opens the JavaScript tab", async ({ page }) => {
  await page.goto("/flick-lang/transpile/");
  await expect(page).toHaveURL(/\/play\/\?tab=js$/);
  await ready(page);
  await expect(page.locator(".fp-tabs .fp-on")).toHaveText("JavaScript");
  await expect(page.locator(".fp-generated-code")).toContainText("@flick-lang/runtime");
});

test("an example from the gallery fails with its code", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await page.locator(".fp-gallery-button").click();
  await page.locator(".fp-gallery").getByRole("button", { name: "A type error" }).click();
  await expect(page.locator(".fp-gallery")).toBeHidden();
  await expect(page.locator(".fp-status")).toContainText("Errors", { timeout: 60_000 });
  await expect(page.locator(".fp-problems-panel")).toContainText("FLICK-T003");
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
  await hoverWord(page, "match status", "status");
  await page.locator(".cm-diagnosticAction").click();
  await expect(page.locator(".cm-content")).toContainText("| Paused ->");
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
});
