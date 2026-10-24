import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { test } from "node:test";

import { Effect, Exit, Fiber, Option } from "effect";
import { JSDOM } from "jsdom";
import { StrictMode, createElement } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";

import { type Failure, describeReport, failureInspect, refMake, refSet, scope } from "../src/index.ts";
import { uiRun } from "../src/ui/index.ts";
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
  uiKeyed,
  uiLazyWith,
  uiRunWith,
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

test("latest does not interrupt the replacement started by synchronous cancellation", { timeout: 1000 }, async () => {
  const log: Array<string> = [];
  const started = Promise.withResolvers<void>();
  let release: (() => void) | undefined;
  let n = 0;
  const run = (value: number) => Effect.gen(function* () {
    log.push(`start ${value}`);
    if (value === 1) yield* Effect.never;
    else yield* Effect.callback<void>((resume) => {
      release = () => resume(Effect.void);
      started.resolve();
    });
    log.push(`end ${value}`);
  });
  const root = container();
  const { fiber } = mount(root, () => eventLatest(() => run(++n)), log);
  try {
    click(root);
    click(root);
    await started.promise;
    assert.ok(release);
    release();
    await tick();
    assert.deepEqual(log, ["start 1", "start 2", "end 2"]);
  } finally {
    await Effect.runPromise(Fiber.interrupt(fiber));
  }
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

test("the React-only package entrypoint imports without the DOM or react-dom", () => {
  execFileSync(
    process.execPath,
    [
      "--input-type=module",
      "--eval",
      `
        import assert from "node:assert/strict";
        import { registerHooks } from "node:module";
        registerHooks({
          resolve(specifier, context, next) {
            if (specifier === "react-dom" || specifier.startsWith("react-dom/")) throw new Error("loaded react-dom");
            return next(specifier, context);
          },
        });
        assert.equal(typeof document, "undefined");
        assert.equal(typeof window, "undefined");
        // Import after installing the hook to exercise the module-loading boundary.
        const core = await import("@flick-lang/runtime/ui/react");
        assert.equal(typeof core.uiRunWith, "function");
        assert.equal(typeof core.uiLazyWith, "function");
      `,
    ],
    { cwd: new URL("..", import.meta.url) },
  );
});

test("a renderer opens only when run and retains live listeners through StrictMode cleanup replay", async () => {
  const root = container();
  const model = refMake(0);
  let opened = 0;
  let closed = 0;
  const program = uiRunWith(
    () => {
      opened++;
      const renderer = createRoot(root);
      let first = true;
      return {
        render: (node) => {
          const tree = createElement(StrictMode, null, node);
          if (first) {
            first = false;
            flushSync(() => renderer.render(tree));
          } else renderer.render(tree);
        },
        unmount: () => {
          closed++;
          renderer.unmount();
        },
      };
    },
    () => Effect.void,
    model,
    (count) => uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, count + 1))) }, [uiText(`${count}`)]),
  );
  assert.equal(opened, 0);
  const fiber = Effect.runFork(program);
  assert.equal(opened, 1);
  assert.equal(root.textContent, "0");
  click(root);
  await tick();
  assert.equal(root.textContent, "1");
  await Effect.runPromise(Fiber.interrupt(fiber));
  assert.equal(closed, 1);
  assert.equal(root.childNodes.length, 0);
});

test("same-model completions skip the view and a synchronous task burst batches changed models", async () => {
  const root = container();
  const model = refMake(0);
  let renders = 0;
  let completed = 0;
  const fiber = Effect.runFork(
    uiRun(root, () => Effect.void, model, (count) => {
      renders++;
      return uiFragment([
        uiButton({ disabled: false, on_click: eventConcurrent(200, () => Effect.sync(() => void completed++)) }, [uiText("noop")]),
        uiButton({ disabled: false, on_click: eventConcurrent(200, () => Effect.sync(() => refSet(model, model.value + 1))) }, [uiText(`${count}`)]),
      ]);
    }),
  );
  assert.equal(renders, 1);
  for (let i = 0; i < 100; i++) click(root);
  await tick();
  assert.equal(completed, 100);
  assert.equal(renders, 1);
  for (let i = 0; i < 100; i++) click(root, 1);
  await tick();
  assert.equal(model.value, 100);
  assert.equal(root.textContent, "noop100");
  assert.equal(renders, 2);
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test("lazy views compare slices with the current comparator and replace changed view closures", async () => {
  const root = container();
  const model = refMake({ slice: 1, noise: 0, replace: false, force: false });
  let renders = 0;
  const view = (state: typeof model.value): Ui => {
    renders++;
    return uiText(`slice ${state.slice}`);
  };
  const replacement = (state: typeof model.value): Ui => {
    renders++;
    return uiText(`replacement ${state.slice}`);
  };
  const fiber = Effect.runFork(
    uiRun(root, () => Effect.void, model, (state) =>
      uiFragment([
        uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, { ...state, noise: state.noise + 1 }))) }, []),
        uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, { ...state, slice: state.slice + 1 }))) }, []),
        uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, { ...state, replace: true }))) }, []),
        uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, { ...state, force: true }))) }, []),
        uiLazyWith((a, b) => !state.force && a.slice === b.slice, state.replace ? replacement : view, state),
      ]),
    ),
  );
  assert.equal(renders, 1);
  click(root);
  await tick();
  assert.equal(renders, 1);
  click(root, 1);
  await tick();
  assert.equal(root.textContent, "slice 2");
  assert.equal(renders, 2);
  click(root, 2);
  await tick();
  assert.equal(root.textContent, "replacement 2");
  assert.equal(renders, 3);
  click(root, 3);
  await tick();
  assert.equal(renders, 4);
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test("keyed lazy siblings keep distinct caches, DOM nodes and busy exhaust policies while reordering", async () => {
  const root = container();
  const model = refMake(false);
  const renders: Array<string> = [];
  const starts: Array<string> = [];
  const view = (id: string): Ui => {
    renders.push(id);
    return uiButton(
      { disabled: false, on_click: eventExhaust(() => Effect.sync(() => starts.push(id)).pipe(Effect.andThen(Effect.never))) },
      [uiText(id)],
    );
  };
  const fiber = Effect.runFork(
    uiRun(root, () => Effect.void, model, (reversed) =>
      uiFragment([
        uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, !reversed))) }, [uiText("reverse")]),
        ...(reversed ? ["b", "a"] : ["a", "b"]).map((id) => uiKeyed(id, uiLazyWith((a, b) => a === b, view, id))),
      ]),
    ),
  );
  const a = root.querySelectorAll("button")[1];
  const b = root.querySelectorAll("button")[2];
  click(root, 1);
  click(root);
  await tick();
  assert.equal(root.querySelectorAll("button")[1], b);
  assert.equal(root.querySelectorAll("button")[2], a);
  assert.deepEqual(renders, ["a", "b"]);
  click(root, 2);
  click(root, 1);
  await tick();
  assert.deepEqual(starts, ["a", "b"]);
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test("keys containing path separators cannot alias nested lazy caches or event policies", async () => {
  const root = container();
  const model = refMake(0);
  const starts: Array<string> = [];
  const renders: Array<string> = [];
  const view = (id: string): Ui => {
    renders.push(id);
    return uiButton(
      { disabled: false, on_click: eventExhaust(() => Effect.sync(() => starts.push(id)).pipe(Effect.andThen(Effect.never))) },
      [uiText(id)],
    );
  };
  const fiber = Effect.runFork(
    uiRun(root, () => Effect.void, model, (count) =>
      uiFragment([
        uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, count + 1))) }, []),
        uiKeyed("a", uiFragment([uiKeyed("b", uiLazyWith(() => true, view, "nested"))])),
        uiKeyed('a/k:"b"', uiLazyWith(() => true, view, "flat")),
      ]),
    ),
  );
  click(root, 1);
  click(root, 2);
  click(root);
  await tick();
  assert.deepEqual(starts, ["nested", "flat"]);
  assert.deepEqual(renders, ["nested", "flat"]);
  await Effect.runPromise(Fiber.interrupt(fiber));
});

test("removing an event drops its waiting queue but started tasks remain owned until root cleanup", async () => {
  const root = container();
  const model = refMake(true);
  const log: Array<string> = [];
  const { run, release } = gated(log);
  let n = 0;
  const fiber = Effect.runFork(
    uiRun(root, () => Effect.void, model, (visible) =>
      uiFragment([
        uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, !visible))) }, []),
        ...(visible ? [uiKeyed("task", uiButton({ disabled: false, on_click: eventQueue(1, () => run(++n)) }, []))] : []),
      ]),
    ),
  );
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
  await Effect.runPromise(Fiber.interrupt(fiber));
  assert.deepEqual(log, ["start 1", "start 2", "end 1", "stopped 2"]);
});

test("an initial view defect fails the root and removes its mount", async () => {
  const root = container();
  const exit = await Effect.runPromiseExit(
    uiRun(root, () => Effect.void, refMake(0), () => {
      throw new Error("initial view broke");
    }),
  );
  assert.ok(Exit.isFailure(exit));
  assert.match(String(Exit.isFailure(exit) && exit.cause.reasons.map((r) => (r._tag === "Die" ? String(r.defect) : r._tag))), /initial view broke/);
  assert.equal(root.childNodes.length, 0);
});

test("a later render defect unmounts and joins event tasks before the outer scope releases", async () => {
  const root = container();
  const model = refMake(false);
  const log: Array<string> = [];
  const task = Effect.never.pipe(
    Effect.onInterrupt(() =>
      Effect.promise(() => tick()).pipe(Effect.map(() => void log.push(`task stopped, mounted ${root.childNodes.length > 0}`))),
    ),
  );
  const program = scope(() =>
    Effect.gen(function* () {
      yield* Effect.acquireRelease(Effect.void, () => Effect.sync(() => log.push("resource released")));
      return yield* uiRun(root, () => Effect.void, model, (broken) => {
        if (broken) throw new Error("later view broke");
        return uiFragment([
          uiButton({ disabled: false, on_click: eventExhaust(() => task) }, []),
          uiButton({ disabled: false, on_click: eventExhaust(() => Effect.sync(() => refSet(model, true))) }, []),
        ]);
      });
    }),
  );
  const fiber = Effect.runFork(program);
  click(root);
  click(root, 1);
  const exit = await Effect.runPromise(Fiber.await(fiber));
  assert.ok(Exit.isFailure(exit));
  assert.match(String(Exit.isFailure(exit) && exit.cause.reasons.map((r) => (r._tag === "Die" ? String(r.defect) : r._tag))), /later view broke/);
  assert.deepEqual(log, ["task stopped, mounted false", "resource released"]);
});
