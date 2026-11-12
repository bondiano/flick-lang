import { expect, test } from "@playwright/test";
import { hoverWord, ready, replaceSource } from "./helpers.ts";

test("hovers render Markdown and highlight signatures and documented code", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await replaceSource(page, 'module Main exposing (main)\n\n/// A **friendly** greeting using `name`.\n///\n/// - Keeps the name.\n/// - Returns a greeting.\n///\n/// [Reference][reference]\n///\n/// > [reference]: https://example.com/reference\n///\n/// ```flick\n/// let example = greet "Flick"\n/// ```\ngreet : String -> String\nlet greet name = name\n\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (greet "Flick")\n');
  await hoverWord(page, "let greet", "greet");
  const hover = page.locator(".fp-hover");
  await expect(hover.locator("pre code").first()).toHaveText("greet : String -> String");
  await expect(hover.locator("pre .fp-token-keyword").filter({ hasText: /^String$/ }).first()).toBeVisible();
  await expect(hover.locator("strong")).toHaveText("friendly");
  await expect(hover.locator("p code").first()).toHaveText("name");
  await expect(hover.locator("li")).toHaveCount(2);
  await expect(hover.locator("pre .fp-token-string")).toHaveText('"Flick"');
  await expect(hover.locator("p code").last()).toHaveText("Main");
  await expect(hover.getByRole("link", { name: "Reference" })).toHaveAttribute("href", "https://example.com/reference");
  await expect(hover).not.toContainText("```");
});

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

test("diagnostic hovers format inline code too", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await replaceSource(page, "module Main exposing (main)\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line missing\n");
  await page.locator(".cm-lintRange-error").first().hover();
  await expect(page.locator(".cm-diagnostic code")).toHaveText("missing");
  await expect(page.locator(".cm-diagnostic")).not.toContainText("`missing`");
});
