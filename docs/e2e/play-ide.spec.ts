// The playground's editor features: the semantic tokens, the signature of the
// call under the cursor and the rewrites of the checker, all over the module
// the reader types, including its non-ASCII text. It shows no inlay types.
import { expect, test, type Page } from "@playwright/test";

/** The compiler has loaded and checked the first example, and the editor has opened. */
const ready = async (page: Page): Promise<void> => {
  await expect(page.locator(".fp-status")).toContainText("0.2.0", { timeout: 60_000 });
  await expect(page.locator(".fp-cm .cm-content")).toBeVisible();
};

/** Replace the native editor contents through CodeMirror's normal input path. */
const replaceSource = async (page: Page, source: string): Promise<void> => {
  await page.locator(".cm-content").click();
  await page.keyboard.press("ControlOrMeta+End");
  await page.keyboard.press("ControlOrMeta+a");
  await page.keyboard.insertText(source);
};

const SOURCE = `module Main exposing (main)

double : Int -> Int
let double x = x * 2

main : Unit -> Unit
let main () =
  let text = "déjà 𝄞"
  let doubled = double (21)
  if doubled == 42 then ()
`;

test("the editor shows what the checker knows, over non-ASCII text", async ({ page }) => {
  await page.goto("/flick-lang/play/");
  await ready(page);
  await replaceSource(page, SOURCE);
  await expect(page.locator(".fp-status")).toContainText("Type-checked", { timeout: 30_000 });

  // every resolved name carries the mark of its kind
  await expect(page.locator(".fp-cm .fp-ide-token").first()).toBeVisible({ timeout: 30_000 });

  // the call after the non-ASCII string is where it is written, not where its
  // UTF-8 bytes would put it
  const called = page.locator(".cm-line", { hasText: "double (21)" });
  await expect(called.locator(".fp-ide-function", { hasText: "double" })).toBeVisible();
  // a `let` without a signature shows only what the reader wrote: no inlay type
  await expect(page.locator(".cm-line", { hasText: "let doubled" })).toHaveText("let doubled = double (21)");

  // the signature of the call appears inside its parentheses
  await called.click();
  await page.keyboard.press("End");
  for (let i = 0; i < 3; i++) await page.keyboard.press("ArrowLeft");
  await expect(page.locator(".fp-ide-signature")).toContainText("Int -> Int", { timeout: 30_000 });

  // and the checker's rewrites are a menu away
  await page.locator(".cm-line", { hasText: "déjà" }).first().click({ position: { x: 24, y: 8 } });
  await page.keyboard.press("ControlOrMeta+.");
  const menu = page.locator(".fp-ide-menu");
  await expect(menu).toBeVisible({ timeout: 30_000 });
  await menu.getByRole("button", { name: /add the signature `text : String`/ }).click();
  await expect(page.locator(".cm-content")).toContainText("text : String");
});
