import { expect, test } from "@playwright/test";
import { hoverWord, ready, replaceSource } from "./helpers.ts";

test("mutable local hovers update after an edit", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  const mutable = "module Main exposing (main)\n// 🦔\nmain : Unit ->{} Unit\nlet main () =\n  let mut total : Int = 1\n  total = total + 1\n  Platform.stdout_line (show total)\n";
  const immutable = mutable.replace("let mut total", "let total").replace("  total = total + 1\n", "");
  for (const [source, expected, lines] of [
    [mutable, "mut total : Int", ["let mut total", "total = total + 1", "Platform.stdout_line"]],
    [immutable, "total : Int", ["let total", "Platform.stdout_line"]],
  ] as const) {
    await replaceSource(page, source);
    await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });
    for (const line of lines) {
      await hoverWord(page, line, "total");
      await expect(page.locator(".cm-tooltip-hover .fp-hover")).toHaveText(expected);
      await page.mouse.move(0, 0);
      await expect(page.locator(".cm-tooltip-hover")).toHaveCount(0);
    }
  }
});
