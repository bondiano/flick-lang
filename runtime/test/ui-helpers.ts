// What the UI tests share: a jsdom container, scripted clicks, gated tasks
// and roots rendered through `uiRunWith` with a synchronous React DOM root.
import assert from "node:assert/strict";

import { dom } from "./jsdom.ts";

import { Effect, Fiber } from "effect";
import * as Atom from "effect/reactivity/Atom";
import type * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { flushSync } from "react-dom";
import { createRoot, type Root } from "react-dom/client";

import { type Failure, ReadService, describeReport, failureInspect, syncOperation } from "../src/index.ts";
import { type Renderer, type Ui, uiRunWith } from "../src/ui/react/index.ts";

export { dom };

export const container = (): Element => {
  const div = dom.window.document.createElement("div");
  dom.window.document.body.append(div);
  return div;
};

export const click = (root: Element, at = 0): boolean => {
  const button = root.querySelectorAll("button")[at];
  assert.ok(button, "a button is rendered");
  return button.dispatchEvent(new dom.window.MouseEvent("click", { bubbles: true, cancelable: true }));
};

export const tick = (ms = 5): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

/** A task that logs its start, then waits until `release` or its interruption. */
export const gated = (log: Array<string>) => {
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

/** A React DOM root in `root` whose first render commits at once. */
export const domRenderer =
  (root: Element): Renderer =>
  () => {
    let react: Root | undefined;
    return {
      render: (node) =>
        flushSync(() => {
          react ??= createRoot(root);
          react.render(node);
        }),
      unmount: () => react?.unmount(),
    };
  };

/** A supervisor that logs each failure. */
export const logging = (log: Array<string>) => (failure: Failure) =>
  Effect.map(failureInspect(failure), (report) => void log.push(`supervisor: ${describeReport(report)}`));

/** Runs `ui` in `root`; the supervisor logs into `log`. */
export const mount = (root: Element, ui: Ui, log: Array<string> = [], registry?: AtomRegistry.AtomRegistry) =>
  Effect.runFork(uiRunWith(domRenderer(root), logging(log), ui, registry));

export const stop = (fiber: Fiber.Fiber<unknown, unknown>): Promise<unknown> => Effect.runPromise(Fiber.interrupt(fiber));

/** `Atom.get atom` as a Flick render does it: a Sync call of `Atom.Read`. */
export const get = <A>(atom: Atom.Atom<A>): A =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the read gives the atom's own value
  (syncOperation(ReadService, "get", 1) as (atom: Atom.Atom<A>) => A)(atom);

export { Atom };
