import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Fiber } from "effect";
import { JSDOM } from "jsdom";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";

import {
  type Handler,
  type Renderer,
  type Ui,
  actionExhaust,
  actionLatest,
  actionWith,
  eventExhaust,
  uiButton,
  uiFragment,
  uiRunWith,
  uiText,
} from "../src/ui/react/index.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>");
Object.assign(globalThis, { window: dom.window, document: dom.window.document });

const tick = (ms = 5): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

const mount = (ui: Ui) => {
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
  const fiber = Effect.runFork(uiRunWith(renderer, () => Effect.void, ui));
  const click = (at: number): void => {
    const button = container.querySelectorAll("button")[at];
    assert.ok(button, "a button is rendered");
    button.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true, cancelable: true }));
  };
  return { click, stop: () => Effect.runPromise(Fiber.interrupt(fiber)) };
};

/** Tasks that log their start and wait for `release`, or their interruption. */
const gated = (log: Array<string>) => {
  const gates: Array<() => void> = [];
  let n = 0;
  const run = () =>
    Effect.gen(function* () {
      const id = ++n;
      log.push(`start ${id}`);
      yield* Effect.callback<void>((resume) => {
        gates.push(() => resume(Effect.void));
      });
      log.push(`end ${id}`);
    }).pipe(Effect.onInterrupt(() => Effect.sync(() => void log.push("stopped"))));
  return { run, release: () => gates.shift()?.() };
};

const two = (left: Handler, right: Handler): Ui =>
  uiFragment([
    uiButton({ disabled: false, on_click: left }, [uiText("a")]),
    uiButton({ disabled: false, on_click: right }, [uiText("b")]),
  ]);

test("the elements using one action share its policy", async () => {
  const log: Array<string> = [];
  const tasks = gated(log);
  const save = actionExhaust(tasks.run);
  const { click, stop } = mount(two(save, save));
  await tick();
  click(0);
  click(1);
  await tick();
  assert.deepEqual(log, ["start 1"], "exhaust ignores the second button while the first runs");
  tasks.release();
  await tick();
  click(1);
  await tick();
  assert.deepEqual(log, ["start 1", "end 1", "start 2"]);
  await stop();
  assert.deepEqual(log, ["start 1", "end 1", "start 2", "stopped"], "the root's end stops the action's task");
});

test("event handlers keep a policy per element", async () => {
  const log: Array<string> = [];
  const tasks = gated(log);
  const { click, stop } = mount(two(eventExhaust(tasks.run), eventExhaust(tasks.run)));
  await tick();
  click(0);
  click(1);
  await tick();
  assert.deepEqual(log, ["start 1", "start 2"]);
  await stop();
});

test("a latest action interrupts its task started from another element", async () => {
  const log: Array<string> = [];
  const tasks = gated(log);
  const search = actionLatest(tasks.run);
  const { click, stop } = mount(two(search, search));
  await tick();
  click(0);
  await tick();
  click(1);
  await tick();
  assert.deepEqual(log, ["start 1", "stopped", "start 2"]);
  await stop();
});

test("`Action.with` passes its data and shares the policy of its action", async () => {
  const seen: Array<unknown> = [];
  const log: Array<string> = [];
  const tasks = gated(log);
  const remove = actionExhaust((pair: unknown) => {
    seen.push(Array.isArray(pair) ? pair[0] : undefined);
    return tasks.run();
  });
  const { click, stop } = mount(two(actionWith(remove, "a"), actionWith(remove, "b")));
  await tick();
  click(1);
  click(0);
  await tick();
  assert.deepEqual(seen, ["b"], "the second button waits for the shared exhaust policy");
  tasks.release();
  await tick();
  click(0);
  await tick();
  assert.deepEqual(seen, ["b", "a"]);
  await stop();
});
