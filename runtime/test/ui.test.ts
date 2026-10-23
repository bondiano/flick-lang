import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Exit, Fiber, Option } from "effect";
import { JSDOM } from "jsdom";

import { type Failure, describeReport, failureInspect, refMake, refSet, scope } from "../src/index.ts";
import {
  type Handler,
  type Ui,
  eventConcurrent,
  eventExhaust,
  eventLatest,
  eventPreventDefault,
  eventQueue,
  uiButton,
  uiFragment,
  uiRun,
  uiSafeUrl,
  uiText,
} from "../src/ui/react/index.ts";

const dom = new JSDOM("<!doctype html><html><body></body></html>");
Object.assign(globalThis, { window: dom.window, document: dom.window.document });

const container = (): Element => {
  const div = dom.window.document.createElement("div");
  dom.window.document.body.append(div);
  return div;
};

const click = (root: Element, at = 0): boolean => {
  const button = root.querySelectorAll("button")[at];
  assert.ok(button, "a button is rendered");
  return button.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true, cancelable: true }));
};

const tick = (ms = 5): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** A task that logs its start, then waits until `release` or its interruption. */
const gated = (log: Array<string>) => {
  const gates: Array<() => void> = [];
  const run = (n: number) =>
    Effect.gen(function* () {
      log.push(`start ${n}`);
      yield* Effect.callback<void>((resume) => {
        gates.push(() => resume(Effect.void));
      });
      log.push(`end ${n}`);
    }).pipe(Effect.onInterrupt(() => Effect.promise(() => tick(10)).pipe(Effect.map(() => void log.push(`stopped ${n}`)))));
  return { run, release: () => gates.shift()?.() };
};

/** Mounts `view` of a counter of events; the supervisor logs each failure. */
const mount = (root: Element, handler: (events: number) => Handler, log: Array<string>) => {
  const model = refMake(0);
  const supervisor = (failure: Failure) =>
    Effect.map(failureInspect(failure), (report) => void log.push(`supervisor: ${describeReport(report)}`));
  const view = (count: number): Ui => uiButton({ disabled: false, on_click: handler(count) }, [uiText(`clicked ${count}`)]);
  return { model, fiber: Effect.runFork(uiRun(root, supervisor, model, view)) };
};

test("text goes through the text channel; unsafe URLs are refused", async () => {
  const root = container();
  const fiber = Effect.runFork(uiRun(root, () => Effect.void, refMake(0), () => uiFragment([uiText("<script>alert(1)</script>")])));
  await tick();
  assert.equal(root.textContent, "<script>alert(1)</script>");
  assert.equal(root.querySelector("script"), null);
  await Effect.runPromise(Fiber.interrupt(fiber));
  assert.equal(uiSafeUrl("javascript:alert(1)")._tag, "None");
  assert.equal(uiSafeUrl("JAVA\tSCRIPT:alert(1)")._tag, "None");
  assert.equal(uiSafeUrl("data:text/html,x")._tag, "None");
  assert.deepEqual(uiSafeUrl("https://example.com/a?b"), Option.some("https://example.com/a?b"));
  assert.deepEqual(uiSafeUrl("/users/1"), Option.some("/users/1"));
});

test("exhaust ignores events while its task runs", async () => {
  const log: Array<string> = [];
  const { run, release } = gated(log);
  let n = 0;
  const root = container();
  const { fiber } = mount(root, () => eventExhaust(() => run(++n)), log);
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
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test("latest interrupts the running task, waits until it stopped, then runs the last event", async () => {
  const log: Array<string> = [];
  const { run } = gated(log);
  let n = 0;
  const root = container();
  const { fiber } = mount(root, () => eventLatest(() => run(++n)), log);
  click(root);
  await tick(1);
  click(root);
  click(root);
  await tick(40);
  // the second event was replaced by the third while the first was stopping
  assert.deepEqual(log, ["start 1", "stopped 1", "start 2"]);
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test("queue and concurrent reject an event beyond their bound and tell the supervisor", async () => {
  const log: Array<string> = [];
  const { run, release } = gated(log);
  let n = 0;
  const root = container();
  const queued = mount(root, () => eventQueue(1, () => run(++n)), log);
  click(root);
  click(root);
  click(root);
  await tick();
  assert.deepEqual(log, ["start 1", "supervisor: die error event rejected: the queue of a handler is full: 1 events are waiting"]);
  release();
  await tick();
  assert.deepEqual(log.slice(2), ["end 1", "start 2"]);
  await Effect.runPromise(Fiber.interrupt(queued.fiber));

  log.length = 0;
  const other = container();
  const parallel = mount(other, () => eventConcurrent(2, () => run(++n)), log);
  click(other);
  click(other);
  click(other);
  await tick();
  assert.deepEqual(log, ["start 3", "start 4", "supervisor: die error event rejected: a handler already runs 2 tasks"]);
  await Effect.runPromise(Fiber.interrupt(parallel.fiber));
  assert.throws(() => eventQueue(0, () => Effect.void), RangeError);
});

test("the default action is decided while the browser waits", async () => {
  const root = container();
  const prevent = eventPreventDefault(() => true, eventExhaust(() => Effect.void));
  const { fiber } = mount(root, () => prevent, []);
  await tick();
  assert.equal(click(root), false);
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test("a task's result renders again", async () => {
  const root = container();
  const { model, fiber } = mount(root, (count) => eventExhaust(() => Effect.sync(() => refSet(model, count + 1))), []);
  click(root);
  await tick();
  click(root);
  await tick();
  assert.equal(root.textContent, "clicked 2");
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test("closing stops events at once, joins the tasks, then the scope releases its resources", async () => {
  const log: Array<string> = [];
  const root = container();
  const task = Effect.never.pipe(
    Effect.onInterrupt(() => Effect.sync(() => log.push(`task stopped, mounted ${root.childNodes.length > 0}`))),
  );
  const program = scope(() =>
    Effect.gen(function* () {
      yield* Effect.acquireRelease(Effect.void, () => Effect.sync(() => log.push("resource released")));
      const running = yield* Effect.forkChild(
        uiRun(root, () => Effect.void, refMake(0), () => uiButton({ disabled: false, on_click: eventConcurrent(5, () => task) }, [uiText("go")])),
      );
      yield* Effect.promise(() => tick());
      const button = root.querySelector("button");
      click(root);
      yield* Effect.promise(() => tick());
      yield* Fiber.interrupt(running);
      log.push("run ended");
      // a listener of the unmounted tree does nothing
      button?.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true }));
    }),
  );
  await Effect.runPromise(program);
  assert.deepEqual(log, ["task stopped, mounted false", "run ended", "resource released"]);
});

const failingSupervisor = () => Effect.die(new Error("supervisor broke"));
const dying = (): Ui => uiButton({ disabled: false, on_click: eventExhaust(() => Effect.die(new Error("boom"))) }, []);

test("a failing supervisor ends the root as a defect", async () => {
  const root = container();
  const fiber = Effect.runFork(uiRun(root, failingSupervisor, refMake(0), dying));
  await tick();
  click(root);
  const exit = await Effect.runPromise(Fiber.await(fiber));
  assert.ok(Exit.isFailure(exit));
  assert.match(String(Exit.isFailure(exit) && exit.cause.reasons.map((r) => (r._tag === "Die" ? String(r.defect) : r._tag))), /supervisor broke/);
});
