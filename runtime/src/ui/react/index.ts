// React-only rendering and event ownership. Adapters mount roots; this core
// stops events first, then interrupts and joins tasks before its scope closes.
import { Cause, Effect, Fiber } from "effect";
import { Component, Fragment, type ReactNode, createElement, memo, useLayoutEffect, useMemo } from "react";

import type { Cell } from "../../builtins.ts";
import type { Failure } from "../../cause.ts";
import type { Ui } from "./nodes.ts";
import { Bridge, type Listener } from "./policies.ts";

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

interface RenderRoot {
  render(node: ReactNode): void;
  unmount(): void;
}

/** A backend opens its root when the effect runs, not when the description is built. */
export type Renderer = () => RenderRoot;

type Events = Pick<Bridge<never>, "listener">;
type ElementNode = Extract<Ui, { readonly kind: "element" }>;
type LazyNode = Extract<Ui, { readonly kind: "lazy" }>;

const EventElement = ({ bridge, node }: { readonly bridge: Events; readonly node: ElementNode }): ReactNode => {
  const listeners = useMemo(() => new Map<string, Listener>(), [bridge]);
  const props: Record<string, unknown> = { ...node.attrs };
  for (const [name, binding] of Object.entries(node.events)) {
    let listener = listeners.get(name);
    if (listener === undefined) {
      listener = bridge.listener(binding);
      listeners.set(name, listener);
    }
    props[name] = listener.callback;
  }
  // Publish new closures only after React commits; an abandoned render cannot change a live listener.
  useLayoutEffect(() => {
    for (const [name, listener] of listeners) {
      const binding = node.events[name];
      if (binding === undefined) {
        listener.close();
        listeners.delete(name);
      } else listener.update(binding);
    }
  });
  useLayoutEffect(
    () => () => {
      for (const listener of listeners.values()) listener.close();
    },
    [listeners],
  );
  if (node.html === undefined) return createElement(node.tag, props, ...children(bridge, node.children));
  props["dangerouslySetInnerHTML"] = { __html: node.html };
  return createElement(node.tag, props);
};

const LazyView = memo(
  ({ bridge, node }: { readonly bridge: Events | undefined; readonly node: LazyNode }): ReactNode => toReact(bridge, node.view(node.model)),
  (before, after) =>
    before.bridge === after.bridge &&
    before.node.view === after.node.view &&
    after.node.equal(before.node.model, after.node.model),
);

const toReact = (bridge: Events | undefined, node: Ui): ReactNode => {
  if (node.kind === "text") return node.value;
  if (node.kind === "fragment") return createElement(Fragment, null, ...children(bridge, node.children));
  if (node.kind === "keyed") return createElement(Fragment, { key: node.key }, toReact(bridge, node.node));
  if (node.kind === "lazy") return createElement(LazyView, { bridge, node });
  if (node.kind === "component") return createElement(node.component, node.props);
  if (bridge !== undefined && Object.keys(node.events).length > 0) return createElement(EventElement, { bridge, node });
  const props: Record<string, unknown> = { ...node.attrs };
  if (node.html === undefined) return createElement(node.tag, props, ...children(bridge, node.children));
  props["dangerouslySetInnerHTML"] = { __html: node.html };
  return createElement(node.tag, props);
};

const children = (bridge: Events | undefined, nodes: ReadonlyArray<Ui>): Array<ReactNode> => nodes.map((node) => toReact(bridge, node));

/** The same inert view for server markup; event bindings are attached by the client root. */
export const uiRenderStatic = (node: Ui): ReactNode => toReact(undefined, node);

class RenderBoundary extends Component<
  { readonly broken: (cause: Cause.Cause<unknown>) => void; readonly children: ReactNode },
  { readonly failed: boolean }
> {
  override state = { failed: false };

  static getDerivedStateFromError(): { readonly failed: boolean } {
    return { failed: true };
  }

  override componentDidCatch(error: unknown): void {
    // Effect resumes synchronously; unmount only after React has finished this commit.
    queueMicrotask(() => this.props.broken(Cause.die(error)));
  }

  override render(): ReactNode {
    return this.state.failed ? null : this.props.children;
  }
}

/** Runs a pure view through a React renderer; task completions request batched renders only when the model changes. */
export const uiRunWith = <M, R>(
  renderer: Renderer,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  model: Cell<M>,
  view: (model: M) => Ui,
): Effect.Effect<never, unknown, R> =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const context = yield* Effect.context<R>();
      let root: RenderRoot | undefined;
      let failure: Cause.Cause<unknown> | undefined;
      let resumeFailure: ((cause: Cause.Cause<unknown>) => void) | undefined;
      const broken = (cause: Cause.Cause<unknown>): void => {
        if (failure !== undefined) return;
        failure = cause;
        resumeFailure?.(cause);
      };
      let rendered = false;
      let previous: M | undefined;
      const render = (): void => {
        const current = model.value;
        if (!bridge.accepting || failure !== undefined || (rendered && Object.is(previous, current))) return;
        previous = current;
        rendered = true;
        try {
          root?.render(createElement(RenderBoundary, { broken, children: createElement(View, { value: current }) }));
        } catch (error) {
          broken(Cause.die(error));
        }
      };
      const bridge = new Bridge(context, supervisor, render, broken);
      const View = memo(({ value }: { readonly value: M }): ReactNode => toReact(bridge, view(value)));
      const run = Effect.gen(function* () {
        root = yield* Effect.sync(renderer);
        render();
        return yield* restore(
          Effect.callback<never, unknown>((resume) => {
            resumeFailure = (cause) => resume(Effect.failCause(cause));
            if (failure !== undefined) resumeFailure(failure);
          }),
        );
      });
      return yield* Effect.ensuring(
        run,
        Effect.gen(function* () {
          bridge.accepting = false;
          const unmounted = yield* Effect.exit(Effect.sync(() => root?.unmount()));
          yield* Fiber.interruptAll([...bridge.tasks]);
          yield* Fiber.awaitAll([...bridge.supervisors]);
          return yield* unmounted;
        }),
      );
    }),
  );
