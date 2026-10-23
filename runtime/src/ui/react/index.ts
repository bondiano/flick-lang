// The React adapter of `Ui`: `Ui.run` renders a description with React 19
// and runs the tasks of its events through a bridge. Closing is two-phase:
// React cleanup stops events and removes listeners at once; then the tasks
// are interrupted and joined, and only then `Ui.run` ends, so the scope
// around it releases its resources after them.
import { type Cause, Effect, Fiber } from "effect";
import { Fragment, type ReactNode, createElement } from "react";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";

import type { Cell } from "../../builtins.ts";
import type { Failure } from "../../cause.ts";
import type { Ui } from "./nodes.ts";
import { Bridge } from "./policies.ts";

export * from "./nodes.ts";
export {
  eventConcurrent,
  eventExhaust,
  eventLatest,
  eventPreventDefault,
  eventQueue,
  type Handler,
  type Policy,
} from "./policies.ts";

/**
 * The React node of `node`; `place` names its position, so the policy
 * state of a handler survives a render of the same tree.
 */
const toReact = <R>(bridge: Bridge<R>, node: Ui, place: string, key?: string): ReactNode => {
  if (node.kind === "text") {
    return key === undefined ? node.value : createElement(Fragment, { key }, node.value);
  }
  if (node.kind === "fragment") {
    return createElement(Fragment, key === undefined ? null : { key }, ...children(bridge, node.children, place));
  }
  if (node.kind === "keyed") return toReact(bridge, node.node, `${place}/#${node.key}`, node.key);
  if (node.kind === "component") return createElement(node.component, key === undefined ? node.props : { ...node.props, key });
  const props: Record<string, unknown> = { ...node.attrs };
  if (key !== undefined) props["key"] = key;
  for (const [name, binding] of Object.entries(node.events)) {
    props[name] = bridge.listener(`${place}:${name}`, binding);
  }
  if (node.html === undefined) return createElement(node.tag, props, ...children(bridge, node.children, place));
  props["dangerouslySetInnerHTML"] = { __html: node.html };
  return createElement(node.tag, props);
};

const children = <R>(bridge: Bridge<R>, nodes: ReadonlyArray<Ui>, place: string): Array<ReactNode> =>
  nodes.map((child, i) => toReact(bridge, child, `${place}/${i}`));

/**
 * `Ui.run container supervisor model view`: renders `view model` and again
 * after each handler task; runs until interrupted, then closes in two
 * phases. A failure of `supervisor` ends it as a defect.
 */
export const uiRun = <M, R>(
  container: Element,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  model: Cell<M>,
  view: (model: M) => Ui,
): Effect.Effect<never, unknown, R> =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const context = yield* Effect.context<R>();
      let broken: ((cause: Cause.Cause<unknown>) => void) | undefined;
      const root = createRoot(container);
      const render = (): void => flushSync(() => root.render(toReact(bridge, view(model.value), "")));
      const bridge = new Bridge(context, supervisor, render, (cause) => broken?.(cause));
      render();
      const exit = yield* Effect.exit(
        restore(
          Effect.callback<never, unknown>((resume) => {
            broken = (cause) => resume(Effect.failCause(cause));
          }),
        ),
      );
      bridge.accepting = false;
      root.unmount();
      yield* Fiber.interruptAll([...bridge.tasks]);
      yield* Fiber.awaitAll([...bridge.supervisors]);
      return yield* exit;
    }),
  );
