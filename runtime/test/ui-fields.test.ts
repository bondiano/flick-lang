import assert from "node:assert/strict";
import { test } from "node:test";

import { dom } from "./jsdom.ts";

import { Effect, Fiber, Option } from "effect";
import * as Atom from "effect/reactivity/Atom";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";

import { type Renderer, type Ui, eventLatest, uiField, uiFieldArea, uiRunWith } from "../src/ui/react/index.ts";

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
  const fiber = Effect.runFork(uiRunWith(renderer, () => Effect.void, { value: ui }, (node: Ui) => node, registry));
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
