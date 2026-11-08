import assert from "node:assert/strict";
import { test } from "node:test";

import { dom } from "./jsdom.ts";

import { Effect, Fiber, Schema } from "effect";
import * as Atom from "effect/reactivity/Atom";
import { renderToString } from "react-dom/server";

import { RegistryService, flickLayer } from "../src/effect/reactivity/Atom.ts";
import { ReadService, syncOperation } from "../src/index.ts";
import { uiMount } from "../src/ui/index.ts";
import { type Ui, eventLatest, uiButton, uiFragment, uiRender, uiRenderStatic, uiText, uiTextarea, uiView } from "../src/ui/react/index.ts";

/** `Atom.get` as a Flick render calls it: a Sync operation of `Atom.Read`. */
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the operation takes the atom
const get = syncOperation(ReadService, "get", 1) as (atom: unknown) => unknown;

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
  const count = Atom.make(0);
  const add = eventLatest(
    // a Flick handler names its services in its type; the root provides them
    () => Effect.flatMap(Effect.service(RegistryService), (registry) => registry.update(count)((n) => n + 1)),
  );
  const tree = (n: number): Ui =>
    uiFragment([
      uiTextarea({ value: "</textarea><script>not code</script>", placeholder: "", on_input: eventLatest(() => Effect.void) }),
      uiButton({ disabled: false, on_click: add }, [uiText(`clicked ${n}`)]),
    ]);
  const view = uiView(() =>
    tree(Number(get(count))),
  );
  // the server renders the same elements the view renders on the client
  root.innerHTML = renderToString(uiRenderStatic(tree(0)));
  assert.equal(root.querySelector("script"), null);
  const editor = root.querySelector("textarea");
  assert.ok(editor);
  editor.value = "typed before the compiler loaded";
  editor.focus();
  editor.setSelectionRange(3, 8);
  const mounted = Effect.provide(uiMount(root, () => Effect.void, uiRender(view, undefined)), flickLayer(RegistryService));
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the root's one service
  const fiber = Effect.runFork(mounted as Effect.Effect<never, unknown>);
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

test("a cold initial view renders statically, reading atoms from a registry of the call", () => {
  const count = Atom.make(41);
  const view = uiView(() => uiFragment([uiText(`count ${Number(get(count)) + 1}`)]));
  // the docs pattern: `uiRenderStatic` and `renderToString`, with no registry of `Ui.Dom.mount` or `Ui.Server.render`
  const markup = renderToString(uiRenderStatic(uiRender(view, undefined)));
  assert.match(markup, /count 42/);
});


test("each root hydrates the state script of its own container into its own registry", async () => {
  const count = Atom.serializable(Atom.make(0), { key: "count", schema: Schema.Number });
  const view = uiView(() =>
    uiText(`count ${String(get(count))}`),
  );
  const roots = ["first", "second"].map((id, n) => {
    const root = container();
    root.id = id;
    root.setAttribute("data-flick-ssr", "");
    root.innerHTML = `count ${String(n + 1)}`;
    root.insertAdjacentHTML(
      "afterend",
      `<script type="application/json" id="flick-state-${id}">[{"key":"count","value":${String(n + 1)}}]</script>`,
    );
    return root;
  });
  const fibers = roots.map((root) =>
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides the root's one service
    Effect.runFork(Effect.provide(uiMount(root, () => Effect.void, uiRender(view, undefined)), flickLayer(RegistryService)) as Effect.Effect<never, unknown>),
  );
  try {
    await new Promise((resolve) => setTimeout(resolve, 20));
    assert.deepEqual(roots.map((root) => root.textContent), ["count 1", "count 2"]);
    assert.equal(dom.window.document.querySelector('script[type="application/json"]'), null);
  } finally {
    await Effect.runPromise(Fiber.interruptAll(fibers));
  }
});
