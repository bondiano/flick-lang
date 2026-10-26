// Components with a setup: each mounted instance sets up once, StrictMode
// included, keeps its own state and actions, and an instance that unmounts
// interrupts the tasks of its actions while the root still waits for them.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Atom, click, container, gated, get, logging, mount, stop, tick } from "./ui-helpers.ts";

import { Effect } from "effect";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { StrictMode, createElement } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";

import { atomState } from "../src/index.ts";
import { actionExhaust, uiButton, uiFragment, uiRender, uiRunWith, uiStateful, uiText, uiView } from "../src/ui/react/index.ts";

/** A counter whose setup makes its own count and its action. */
const counter = (registry: AtomRegistry.AtomRegistry, setups: { count: number }) =>
  uiStateful((start) => {
    setups.count++;
    const count = atomState(Number(start));
    const add = actionExhaust(() => Effect.sync(() => registry.update(count, (n) => n + 1)));
    return () => uiButton({ disabled: false, on_click: add }, [uiText(`${get(count)}`)]);
  });

test("each instance sets up once under StrictMode and keeps its own state", async () => {
  const root = container();
  const registry = AtomRegistry.make();
  const setups = { count: 0 };
  const view = counter(registry, setups);
  const program = uiRunWith(
    () => {
      const renderer = createRoot(root);
      return {
        render: (node) => flushSync(() => renderer.render(createElement(StrictMode, null, node))),
        unmount: () => renderer.unmount(),
      };
    },
    logging([]),
    uiFragment([uiRender(view, 1), uiRender(view, 10)]),
    registry,
  );
  const fiber = Effect.runFork(program);
  assert.equal(setups.count, 2);
  assert.equal(root.textContent, "110");
  click(root, 0);
  await tick();
  click(root, 1);
  click(root, 1);
  await tick();
  // StrictMode's replayed effects left each instance one live action
  assert.equal(root.textContent, "212");
  assert.equal(setups.count, 2);
  await stop(fiber);
});

test("an instance that unmounts interrupts the tasks of its actions", async () => {
  const root = container();
  const registry = AtomRegistry.make();
  const shown = Atom.make(true);
  const log: Array<string> = [];
  const { run } = gated(log);
  const busy = uiStateful(() => {
    const work = actionExhaust(() => run(1));
    return () => uiButton({ disabled: false, on_click: work }, [uiText("work")]);
  });
  const page = uiView(() => (get(shown) ? uiRender(busy, undefined) : uiText("gone")));
  const fiber = mount(root, uiRender(page, undefined), log, registry);
  click(root);
  await tick();
  assert.deepEqual(log, ["start 1"]);
  registry.set(shown, false);
  await tick(30);
  assert.equal(root.textContent, "gone");
  assert.deepEqual(log, ["start 1", "stopped 1"]);
  await stop(fiber);
});

test("a root that closes waits for the tasks an unmounted instance interrupted", async () => {
  const root = container();
  const registry = AtomRegistry.make();
  const shown = Atom.make(true);
  const log: Array<string> = [];
  const { run } = gated(log);
  const busy = uiStateful(() => {
    const work = actionExhaust(() => run(1));
    return () => uiButton({ disabled: false, on_click: work }, [uiText("work")]);
  });
  const page = uiView(() => (get(shown) ? uiRender(busy, undefined) : uiText("gone")));
  const fiber = mount(root, uiRender(page, undefined), log, registry);
  click(root);
  await tick();
  registry.set(shown, false);
  // the interruption takes 10 ms to finish; the root closes meanwhile
  await stop(fiber);
  assert.deepEqual(log, ["start 1", "stopped 1"]);
});
