import assert from "node:assert/strict";
import { test } from "node:test";

import { dom } from "./jsdom.ts";
import { gated, get } from "./ui-helpers.ts";

import { Effect, Fiber, Option } from "effect";
import * as Atom from "effect/reactivity/Atom";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";

import { type Renderer, type Ui, eventLatest, eventQueue, uiField, uiFieldArea, uiRender, uiRunWith, uiView } from "../src/ui/react/index.ts";

const tick = (ms = 5): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

const mount = (ui: Ui, registry: AtomRegistry.AtomRegistry) => {
  const container = dom.window.document.createElement("div");
  dom.window.document.body.append(container);
  let root: Root | undefined;
  const renderer: Renderer = () => ({
    render: (node) =>
      flushSync(() => {
        root ??= createRoot(container);
        root.render(node);
      }),
    unmount: () => root?.unmount(),
  });
  const fiber = Effect.runFork(uiRunWith(renderer, () => Effect.void, ui, registry));
  return { container, stop: () => Effect.runPromise(Fiber.interrupt(fiber)) };
};

/** Types `value` into `field` as a browser does: the DOM value changes, then `input` fires. */
const type = (field: HTMLInputElement | HTMLTextAreaElement, value: string, caret: number): void => {
  const proto = field instanceof dom.window.HTMLTextAreaElement ? dom.window.HTMLTextAreaElement.prototype : dom.window.HTMLInputElement.prototype;
  // the prototype's setter: React tracks what its own instance setter is given
  Reflect.set(proto, "value", value, field);
  field.setSelectionRange(caret, caret);
  flushSync(() => {
    field.dispatchEvent(new dom.window.Event("input", { bubbles: true }));
  });
};

const inputOf = (container: Element): HTMLInputElement => {
  const input = container.querySelector("input");
  assert.ok(input instanceof dom.window.HTMLInputElement, "an input is rendered");
  return input;
};

test("a field writes each keystroke to its state and keeps the caret", async () => {
  const registry = AtomRegistry.make();
  const query = Atom.make("abc");
  const { container, stop } = mount(uiField({ bind: query, placeholder: "find", on_input: Option.none() }), registry);
  await tick();
  const input = inputOf(container);
  assert.equal(input.value, "abc");
  // an `x` typed after the `a`
  type(input, "axbc", 2);
  assert.equal(registry.get(query), "axbc");
  assert.equal(input.value, "axbc");
  assert.equal(input.selectionStart, 2, "the caret stays after the typed character");
  await stop();
});

test("an IME composition ends in the state, and the field follows it", async () => {
  const registry = AtomRegistry.make();
  const name = Atom.make("");
  const { container, stop } = mount(uiFieldArea({ bind: name, placeholder: "", on_input: Option.none() }), registry);
  await tick();
  const area = container.querySelector("textarea");
  assert.ok(area instanceof dom.window.HTMLTextAreaElement);
  area.dispatchEvent(new dom.window.CompositionEvent("compositionstart", { bubbles: true }));
  type(area, "k", 1);
  type(area, "か", 1);
  area.dispatchEvent(new dom.window.CompositionEvent("compositionend", { bubbles: true, data: "か" }));
  type(area, "かな", 2);
  assert.equal(registry.get(name), "かな");
  assert.equal(area.value, "かな");
  await stop();
});

test("a write to the state from elsewhere shows in the field", async () => {
  const registry = AtomRegistry.make();
  const query = Atom.make("draft");
  const { container, stop } = mount(uiField({ bind: query, placeholder: "", on_input: Option.none() }), registry);
  await tick();
  flushSync(() => registry.set(query, ""));
  await tick();
  assert.equal(inputOf(container).value, "");
  await stop();
});

test("`on_input` runs as a task after the state has the text", async () => {
  const registry = AtomRegistry.make();
  const query = Atom.make("");
  const seen: Array<string> = [];
  const search = eventLatest((snapshot: unknown) =>
    Effect.sync(() => {
      const value: unknown = typeof snapshot === "object" && snapshot !== null ? Reflect.get(snapshot, "value") : undefined;
      seen.push(`${String(value)}/${registry.get(query)}`);
    }),
  );
  const { container, stop } = mount(uiField({ bind: query, placeholder: "", on_input: Option.some(search) }), registry);
  await tick();
  type(inputOf(container), "fl", 2);
  await tick();
  assert.deepEqual(seen, ["fl/fl"]);
  await stop();
});

for (const [tag, field] of [["input", uiField], ["textarea", uiFieldArea]] as const) {
  test(`${tag} toggles its input handler without remounting or retaining removed events`, async () => {
    const registry = AtomRegistry.make();
    const query = Atom.make("abc");
    const enabled = Atom.make(false);
    const log: Array<string> = [];
    const { run, release } = gated(log);
    let calls = 0;
    const handler = eventQueue(1, () => run(++calls));
    const view = uiView(() =>
      Effect.map(get(enabled), (active) =>
        field({ bind: query, placeholder: "", on_input: active ? Option.some(handler) : Option.none() }),
      ),
    );
    const { container, stop } = mount(uiRender(view, undefined), registry);
    try {
      await tick();
      const node = container.querySelector(tag);
      assert.ok(node instanceof dom.window.HTMLInputElement || node instanceof dom.window.HTMLTextAreaElement);
      node.focus();
      type(node, "axbc", 2);
      assert.equal(registry.get(query), "axbc");
      assert.deepEqual(log, []);

      flushSync(() => registry.set(enabled, true));
      assert.equal(container.querySelector(tag), node);
      assert.equal(dom.window.document.activeElement, node);
      assert.equal(node.selectionStart, 2);
      type(node, "axybc", 3);
      type(node, "axyzbc", 4);
      await tick();
      assert.deepEqual(log, ["start 1"]);

      node.dispatchEvent(new dom.window.CompositionEvent("compositionstart", { bubbles: true }));
      flushSync(() => registry.set(enabled, false));
      assert.equal(container.querySelector(tag), node);
      assert.equal(dom.window.document.activeElement, node);
      assert.equal(node.selectionStart, 4);
      type(node, "か", 1);
      node.dispatchEvent(new dom.window.CompositionEvent("compositionend", { bubbles: true, data: "か" }));
      type(node, "かな", 2);
      assert.equal(registry.get(query), "かな");
      assert.equal(node.selectionStart, 2);
      release();
      await tick();
      assert.deepEqual(log, ["start 1", "end 1"], "removing the listener drops its queued event");

      flushSync(() => registry.set(enabled, true));
      assert.equal(container.querySelector(tag), node);
      assert.equal(node.selectionStart, 2);
      type(node, "かなた", 3);
      await tick();
      assert.deepEqual(log, ["start 1", "end 1", "start 2"]);
      assert.equal(registry.get(query), "かなた");
      assert.equal(node.selectionStart, 3);
    } finally {
      await stop();
    }
  });

  test(`${tag} preserves its input policy while replacing a present handler`, async () => {
    const registry = AtomRegistry.make();
    const query = Atom.make("");
    const revision = Atom.make(0);
    const log: Array<string> = [];
    const { run, release } = gated(log);
    const view = uiView(() =>
      Effect.map(get(revision), (current) =>
        field({ bind: query, placeholder: "", on_input: Option.some(eventQueue(1, () => run(current))) }),
      ),
    );
    const { container, stop } = mount(uiRender(view, undefined), registry);
    try {
      await tick();
      const node = container.querySelector(tag);
      assert.ok(node instanceof dom.window.HTMLInputElement || node instanceof dom.window.HTMLTextAreaElement);
      type(node, "a", 1);
      flushSync(() => registry.set(revision, 1));
      assert.equal(container.querySelector(tag), node);
      assert.equal(node.selectionStart, 1);
      type(node, "ab", 2);
      await tick();
      assert.deepEqual(log, ["start 0"], "the replacement waits behind the already running task");
      release();
      await tick();
      assert.deepEqual(log, ["start 0", "end 0", "start 1"], "the next event uses the new handler");
      assert.equal(container.querySelector(tag), node);
      assert.equal(node.selectionStart, 2);
    } finally {
      await stop();
    }
  });
}
