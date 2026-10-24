// React-only rendering and event ownership. Adapters mount roots; this core
// stops events first, then interrupts and joins tasks before its scope closes.
import { Cause, Effect, Fiber } from "effect";
import type * as Atom from "effect/reactivity/Atom";
import type * as AtomRegistry from "effect/reactivity/AtomRegistry";
import {
  Component,
  type FunctionComponent,
  Fragment,
  type ReactNode,
  createContext,
  createElement,
  memo,
  useContext,
  useLayoutEffect,
  useMemo,
  useReducer,
} from "react";

import { runReading } from "../../atoms.ts";

import type { Cell } from "../../builtins.ts";
import type { Failure } from "../../cause.ts";
import type { Ui, View } from "./nodes.ts";
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

const LazyView = /*#__PURE__*/ memo(
  ({ bridge, node }: { readonly bridge: Events | undefined; readonly node: LazyNode }): ReactNode => toReact(bridge, node.view(node.model)),
  (before, after) =>
    before.bridge === after.bridge &&
    before.node.view === after.node.view &&
    after.node.equal(before.node.model, after.node.model),
);

/** The registry views read atoms from: `Ui.Dom.mount` provides it. */
const RegistryContext = /*#__PURE__*/ createContext<AtomRegistry.AtomRegistry | undefined>(undefined);

interface ViewProps {
  readonly bridge: Events | undefined;
  readonly props: unknown;
}

/** One React component per view, named as React's tools show it. */
const components = new WeakMap<View, FunctionComponent<ViewProps>>();

const component = (view: View): FunctionComponent<ViewProps> => {
  const known = components.get(view);
  if (known !== undefined) return known;
  const render = ({ bridge, props }: ViewProps): ReactNode => {
    const registry = useContext(RegistryContext);
    if (registry === undefined) throw new Error("a view reads atoms: render it with `Ui.Dom.mount`, which provides the registry");
    const [, rerender] = useReducer((n: number) => n + 1, 0);
    const read = new Map<Atom.Atom<unknown>, unknown>();
    const node = runReading(view.render(props), (atom) => {
      const value = registry.get(atom);
      read.set(atom, value);
      return value;
    });
    // Subscribe after the commit; a write since the render renders again.
    useLayoutEffect(() => {
      const unsubscribe = [...read.keys()].map((atom) => registry.subscribe(atom, rerender));
      for (const [atom, value] of read) {
        if (!Object.is(registry.get(atom), value)) {
          rerender();
          break;
        }
      }
      return () => {
        for (const off of unsubscribe) off();
      };
    });
    return toReact(bridge, node);
  };
  const made = memo(render);
  made.displayName = view.label ?? "FlickView";
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `memo` of a function component is one to render
  const result = made as unknown as FunctionComponent<ViewProps>;
  components.set(view, result);
  return result;
};

const toReact = (bridge: Events | undefined, node: Ui): ReactNode => {
  if (node.kind === "text") return node.value;
  if (node.kind === "view") return createElement(component(node.view), { bridge, props: node.props });
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
  registry?: AtomRegistry.AtomRegistry,
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
          const tree = createElement(RenderBoundary, { broken, children: createElement(View, { value: current }) });
          root?.render(registry === undefined ? tree : createElement(RegistryContext.Provider, { value: registry }, tree));
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
