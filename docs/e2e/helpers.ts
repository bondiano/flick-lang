import { expect, type Page } from "@playwright/test";

/** The compiler has loaded and checked the first example, and the editor has opened. */
export const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-status")).toContainText("0.1.0", { timeout: 60_000 });
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible();
};

/** Replace the native editor contents through CodeMirror's normal input path. */
export const replaceSource = async (page: Page, source: string): Promise<void> => {
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText(source);
};

/** Moves the pointer onto the first `word` in the editor's line that holds `line`. */
export const hoverWord = async (page: Page, line: string, word: string): Promise<void> => {
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
