import assert from "node:assert/strict";
import { test } from "node:test";

import { Atom, click, container, dom, gated, get, mount, stop, tick } from "./ui-helpers.ts";

import { Effect } from "effect";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";

import {
  type Handler,
  eventConcurrent,
  eventExhaust,
  eventLatest,
  eventPreventDefault,
  eventQueue,
  uiButton,
  uiFragment,
  uiKeyed,
  uiRender,
  uiText,
  uiView,
} from "../src/ui/react/index.ts";

const button = (handler: Handler) => uiButton({ disabled: false, on_click: handler }, [uiText("go")]);

test("exhaust ignores events while its task runs", async () => {
  const log: Array<string> = [];
  const { run, release } = gated(log);
  let n = 0;
  const root = container();
  const fiber = mount(root, button(eventExhaust(() => run(++n))), log);
  click(root);
  click(root);
  click(root);
  await tick();
  assert.deepEqual(log, ["start 1"]);
  release();
  await tick();
  click(root);
  await tick();
  assert.deepEqual(log, ["start 1", "end 1", "start 2"]);
  await stop(fiber);
});

test("latest interrupts the running task, waits until it stopped, then runs the last event", async () => {
  const log: Array<string> = [];
  const { run } = gated(log);
  let n = 0;
  const root = container();
  const fiber = mount(root, button(eventLatest(() => run(++n))), log);
  click(root);
  await tick(1);
  click(root);
  click(root);
  await tick(40);
  // the second event was replaced by the third while the first was stopping
  assert.deepEqual(log, ["start 1", "stopped 1", "start 2"]);
  await stop(fiber);
});

test("latest does not interrupt the replacement started by synchronous cancellation", { timeout: 1000 }, async () => {
  const log: Array<string> = [];
  const started = Promise.withResolvers<void>();
  let release: (() => void) | undefined;
  let n = 0;
  const run = (value: number) =>
    Effect.gen(function* () {
      log.push(`start ${value}`);
      if (value === 1) yield* Effect.never;
      else
        yield* Effect.callback<void>((resume) => {
          release = () => resume(Effect.void);
          started.resolve();
        });
      log.push(`end ${value}`);
    });
  const root = container();
  const fiber = mount(root, button(eventLatest(() => run(++n))), log);
  try {
    click(root);
    click(root);
    await started.promise;
    assert.ok(release);
    release();
    await tick();
    assert.deepEqual(log, ["start 1", "start 2", "end 2"]);
  } finally {
    await stop(fiber);
  }
});

test("queue and concurrent reject an event beyond their bound and tell the supervisor", async () => {
  const log: Array<string> = [];
  const { run, release } = gated(log);
  let n = 0;
  const root = container();
  const queued = mount(root, button(eventQueue(1, () => run(++n))), log);
  click(root);
  click(root);
  click(root);
  await tick();
  assert.deepEqual(log, ["start 1", "supervisor: die error event rejected: the queue of a handler is full: 1 events are waiting"]);
  release();
  await tick();
  assert.deepEqual(log.slice(2), ["end 1", "start 2"]);
  await stop(queued);

  log.length = 0;
  const other = container();
  const parallel = mount(other, button(eventConcurrent(2, () => run(++n))), log);
  click(other);
  click(other);
  click(other);
  await tick();
  assert.deepEqual(log, ["start 3", "start 4", "supervisor: die error event rejected: a handler already runs 2 tasks"]);
  await stop(parallel);
  assert.throws(() => eventQueue(0, () => Effect.void), RangeError);
});

test("the default action is decided while the browser waits", async () => {
  const root = container();
  const fiber = mount(root, button(eventPreventDefault(() => true, eventExhaust(() => Effect.void))));
  await tick();
  assert.equal(click(root), false);
  await stop(fiber);
});

test("removing an event drops its waiting queue but started tasks remain owned until root cleanup", async () => {
  const registry = AtomRegistry.make();
  const visible = Atom.make(true);
  const log: Array<string> = [];
  const { run, release } = gated(log);
  let n = 0;
  const toggle = eventExhaust(() => Effect.sync(() => registry.update(visible, (v) => !v)));
  const queued = eventQueue(1, () => run(++n));
  const view = uiView(() => {
    const shown = get(visible);
    return uiFragment([
      uiButton({ disabled: false, on_click: toggle }, []),
      ...(shown ? [uiKeyed("task", uiButton({ disabled: false, on_click: queued }, []))] : []),
    ]);
  });
  const root = container();
  const fiber = mount(root, uiRender(view, undefined), log, registry);
  await tick();
  const detached = root.querySelectorAll("button")[1];
  click(root, 1);
  click(root, 1);
  click(root);
  await tick();
  assert.deepEqual(log, ["start 1"]);
  detached?.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }));
  click(root);
  await tick();
  click(root, 1);
  await tick();
  assert.deepEqual(log, ["start 1", "start 2"]);
  release();
  await tick();
  assert.deepEqual(log, ["start 1", "start 2", "end 1"]);
  await stop(fiber);
  assert.deepEqual(log, ["start 1", "start 2", "end 1", "stopped 2"]);
});
