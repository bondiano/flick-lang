import assert from "node:assert/strict";
import { test } from "node:test";

import { dom } from "./jsdom.ts";

import { Effect, Fiber } from "effect";
import type * as Atom from "effect/reactivity/Atom";
import * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";

import { ReadService, atomPending, atomTask, failureRethrow } from "../src/index.ts";
import { constructorName } from "../src/tags.ts";
import { type Renderer, type Ui, uiRender, uiRunWith, uiSuspended, uiText, uiView } from "../src/ui/react/index.ts";

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
  return { container, fiber, stop: () => Effect.runPromise(Fiber.interrupt(fiber)) };
};

const fields = (value: unknown): object => (typeof value === "object" && value !== null ? value : {});

/** `Atom.ready atom` as the Flick module has it. */
const ready = (atom: Atom.Atom<object>) =>
  Effect.flatMap(Effect.service(ReadService), (read) =>
    Effect.flatMap(read.get(atom), (value): Effect.Effect<unknown, unknown> => {
      const name = constructorName(String(Reflect.get(fields(value), "_tag")));
      if (name === "Loaded") return Effect.succeed(Reflect.get(fields(value), "_0"));
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Failed` holds a `Failure`
      if (name === "Failed") return failureRethrow(Reflect.get(fields(value), "_0") as Parameters<typeof failureRethrow>[0]);
      return atomPending();
    }),
  );

const noop = (): void => undefined;

/** A task that waits for `release`, then gives or fails with `outcome`. */
const gated = (outcome: Effect.Effect<string, string>) => {
  let release = noop;
  const task = Effect.flatMap(
    Effect.callback<void>((resume) => {
      release = () => resume(Effect.void);
    }),
    () => outcome,
  );
  return { task, release: () => release() };
};

test("a suspended view shows its fallback while a task loads, then the value", async () => {
  const registry = AtomRegistry.make();
  const { task, release } = gated(Effect.succeed("ada"));
  const user = atomTask(task);
  const view = uiSuspended(
    () => uiText("failed"),
    uiText("loading…"),
    () => Effect.map(ready(user), (name) => uiText(`hello ${String(name)}`)),
  );
  const { container, stop } = mount(uiRender(view, undefined), registry);
  await tick();
  assert.equal(container.textContent, "loading…");
  flushSync(release);
  await tick();
  assert.equal(container.textContent, "hello ada");
  await stop();
});

test("a failed task shows the failure view with its error", async () => {
  const registry = AtomRegistry.make();
  const { task, release } = gated(Effect.fail("offline"));
  const user = atomTask(task);
  const view = uiSuspended(
    (failure) => uiText(`failed: ${JSON.stringify(failure)}`),
    uiText("loading…"),
    () => Effect.map(ready(user), (name) => uiText(String(name))),
  );
  const { container, stop } = mount(uiRender(view, undefined), registry);
  await tick();
  flushSync(release);
  await tick();
  assert.match(container.textContent ?? "", /^failed/);
  await stop();
});

test("a plain view that reads a loading task fails with what to do", async (t) => {
  t.mock.method(console, "error", () => undefined);
  const registry = AtomRegistry.make();
  const { task } = gated(Effect.succeed("x"));
  const view = uiView(() => Effect.map(ready(atomTask(task)), () => uiText("never")));
  const { fiber } = mount(uiRender(view, undefined), registry);
  const exit = await Effect.runPromise(Fiber.await(fiber));
  assert.match(exit._tag === "Failure" ? String(exit.cause) : "", /Ui\.suspended/);
});
