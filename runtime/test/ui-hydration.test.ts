import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Fiber } from "effect";
import { JSDOM } from "jsdom";
import { renderToString } from "react-dom/server";

import { refMake, refSet } from "../src/index.ts";
import { uiRun } from "../src/ui/index.ts";
import { type Ui, eventLatest, uiButton, uiFragment, uiRenderStatic, uiText, uiTextarea } from "../src/ui/react/index.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>");
Object.assign(globalThis, { window: dom.window, document: dom.window.document });

const container = (): Element => {
  const div = dom.window.document.createElement("div");
  dom.window.document.body.append(div);
  return div;
};

const click = async (root: Element): Promise<void> => {
  const button = root.querySelector("button");
  assert.ok(button, "a button is rendered");
  const { promise, resolve } = Promise.withResolvers<void>();
  const observer = new dom.window.MutationObserver(() => resolve());
  observer.observe(root, { childList: true, characterData: true, subtree: true });
  button.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true, cancelable: true }));
  await promise;
  observer.disconnect();
};

test("hydration keeps text and focus entered before startup, then handles each event once", async () => {
  const root = container();
  root.setAttribute("data-flick-ssr", "");
  const model = refMake(0);
  const view = (count: number): Ui => uiFragment([
    uiTextarea({
      value: "</textarea><script>not code</script>",
      placeholder: "",
      on_input: eventLatest(() => Effect.void),
    }),
    uiButton({ disabled: false, on_click: eventLatest(() => Effect.sync(() => refSet(model, count + 1))) }, [uiText(`clicked ${count}`)]),
  ]);
  root.innerHTML = renderToString(uiRenderStatic(view(0)));
  assert.equal(root.querySelector("script"), null);
  const editor = root.querySelector("textarea");
  assert.ok(editor);
  editor.value = "typed before the compiler loaded";
  editor.focus();
  editor.setSelectionRange(3, 8);
  const fiber = Effect.runFork(uiRun(root, () => Effect.void, model, view));
  try {
    assert.equal(root.querySelector("textarea"), editor);
    assert.equal(editor.value, "typed before the compiler loaded");
    assert.equal(document.activeElement, editor);
    assert.deepEqual([editor.selectionStart, editor.selectionEnd], [3, 8]);
    await click(root);
    assert.equal(root.querySelector("button")?.textContent, "clicked 1");
    await click(root);
    assert.equal(root.querySelector("button")?.textContent, "clicked 2");
  } finally {
    await Effect.runPromise(Fiber.interrupt(fiber));
  }
});

