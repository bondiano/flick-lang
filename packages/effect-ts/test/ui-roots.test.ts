import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { test } from "node:test";

import { Atom, click, container, dom, domRenderer, get, mount, stop, tick } from "./ui-helpers.ts";

import { Effect, Exit, Fiber, Option } from "effect";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { StrictMode, createElement } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";

import { scope } from "../src/index.ts";
import {
  type Ui,
  eventConcurrent,
  eventExhaust,
  uiButton,
  uiFragment,
  uiRender,
  uiRunWith,
  uiSafeUrl,
  uiText,
  uiView,
} from "../src/ui/react/index.ts";

const defectOf = (exit: Exit.Exit<unknown, unknown>): string =>
  String(Exit.isFailure(exit) && exit.cause.reasons.map((r) => (r._tag === "Die" ? String(r.defect) : r._tag)));

test("text goes through the text channel; unsafe URLs are refused", async () => {
  const root = container();
  const fiber = mount(root, uiFragment([uiText("<script>alert(1)</script>")]));
  await tick();
  assert.equal(root.textContent, "<script>alert(1)</script>");
  assert.equal(root.querySelector("script"), null);
  await stop(fiber);
  assert.equal(uiSafeUrl("javascript:alert(1)")._tag, "None");
  assert.equal(uiSafeUrl("JAVA\tSCRIPT:alert(1)")._tag, "None");
  assert.equal(uiSafeUrl("data:text/html,x")._tag, "None");
  assert.deepEqual(uiSafeUrl("https://example.com/a?b"), Option.some("https://example.com/a?b"));
  assert.deepEqual(uiSafeUrl("/users/1"), Option.some("/users/1"));
});

test("a task's write renders the view that read the state again", async () => {
  const registry = AtomRegistry.make();
  const count = Atom.make(0);
  const add = eventExhaust(() => Effect.sync(() => registry.update(count, (n) => n + 1)));
  const view = uiView(() => uiButton({ disabled: false, on_click: add }, [uiText(`clicked ${get(count)}`)]));
  const root = container();
  const fiber = mount(root, uiRender(view, undefined), [], registry);
  await tick();
  click(root);
  await tick();
  click(root);
  await tick();
  assert.equal(root.textContent, "clicked 2");
  await stop(fiber);
});

test("closing stops events at once, joins the tasks, then the scope releases its resources", async () => {
  const log: Array<string> = [];
  const root = container();
  const task = Effect.never.pipe(Effect.onInterrupt(() => Effect.sync(() => log.push(`task stopped, mounted ${root.childNodes.length > 0}`))));
  const program = scope(() =>
    Effect.gen(function* () {
      yield* Effect.acquireRelease(Effect.void, () => Effect.sync(() => log.push("resource released")));
      const running = yield* Effect.forkChild(
        uiRunWith(domRenderer(root), () => Effect.void, uiButton({ disabled: false, on_click: eventConcurrent(5, () => task) }, [uiText("go")])),
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

test("a failing supervisor ends the root as a defect", async () => {
  const root = container();
  const dying = uiButton({ disabled: false, on_click: eventExhaust(() => Effect.die(new Error("boom"))) }, []);
  const fiber = Effect.runFork(
    uiRunWith(
      () => {
        const react = createRoot(root);
        return { render: (node) => flushSync(() => react.render(node)), unmount: () => react.unmount() };
      },
      () => Effect.die(new Error("supervisor broke")),
      dying,
    ),
  );
  await tick();
  click(root);
  const exit = await Effect.runPromise(Fiber.await(fiber));
  assert.ok(Exit.isFailure(exit));
  assert.match(defectOf(exit), /supervisor broke/);
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
        const core = await import("@flick-lang/effect-ts/ui/react");
        assert.equal(typeof core.uiRunWith, "function");
        assert.equal(typeof core.uiView, "function");
      `,
    ],
    { cwd: new URL("..", import.meta.url) },
  );
});

test("a renderer opens only when run and retains live listeners through StrictMode cleanup replay", async () => {
  const root = container();
  const registry = AtomRegistry.make();
  const count = Atom.make(0);
  let opened = 0;
  let closed = 0;
  const add = eventExhaust(() => Effect.sync(() => registry.update(count, (n) => n + 1)));
  const view = uiView(() => uiButton({ disabled: false, on_click: add }, [uiText(`${get(count)}`)]));
  const program = uiRunWith(
    () => {
      opened++;
      const renderer = createRoot(root);
      return {
        render: (node) => flushSync(() => renderer.render(createElement(StrictMode, null, node))),
        unmount: () => {
          closed++;
          renderer.unmount();
        },
      };
    },
    () => Effect.void,
    uiRender(view, undefined),
    registry,
  );
  assert.equal(opened, 0);
  const fiber = Effect.runFork(program);
  assert.equal(opened, 1);
  assert.equal(root.textContent, "0");
  click(root);
  await tick();
  assert.equal(root.textContent, "1");
  await stop(fiber);
  assert.equal(closed, 1);
  assert.equal(root.childNodes.length, 0);
});

test("an initial view defect fails the root and removes its mount", async (t) => {
  t.mock.method(console, "error", () => undefined);
  const root = container();
  const broken = uiView((): Ui => {
    throw new Error("initial view broke");
  });
  const exit = await Effect.runPromise(Fiber.await(mount(root, uiRender(broken, undefined), [], AtomRegistry.make())));
  assert.ok(Exit.isFailure(exit));
  assert.match(defectOf(exit), /initial view broke/);
  assert.equal(root.childNodes.length, 0);
});

test("a later render defect unmounts and joins event tasks before the outer scope releases", async (t) => {
  t.mock.method(console, "error", () => undefined);
  const root = container();
  const registry = AtomRegistry.make();
  const broken = Atom.make(false);
  const log: Array<string> = [];
  const task = Effect.never.pipe(
    Effect.onInterrupt(() => Effect.promise(() => tick()).pipe(Effect.map(() => void log.push(`task stopped, mounted ${root.childNodes.length > 0}`)))),
  );
  const breaking = eventExhaust(() => Effect.sync(() => registry.set(broken, true)));
  const view = uiView(() => {
    if (get(broken)) throw new Error("later view broke");
    return uiFragment([
      uiButton({ disabled: false, on_click: eventExhaust(() => task) }, []),
      uiButton({ disabled: false, on_click: breaking }, []),
    ]);
  });
  const program = scope(() =>
    Effect.gen(function* () {
      yield* Effect.acquireRelease(Effect.void, () => Effect.sync(() => log.push("resource released")));
      return yield* Fiber.join(mount(root, uiRender(view, undefined), [], registry));
    }),
  );
  const fiber = Effect.runFork(program);
  await tick();
  click(root);
  click(root, 1);
  const exit = await Effect.runPromise(Fiber.await(fiber));
  assert.ok(Exit.isFailure(exit));
  assert.match(defectOf(exit), /later view broke/);
  assert.deepEqual(log, ["task stopped, mounted false", "resource released"]);
});
