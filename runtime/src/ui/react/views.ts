// Views and fields on atoms: React components a `Ui` holds as foreign
// components do, so a program that renders none keeps none of this code.
// They take the root's events and registry from React context.
import { Cause, Effect, Option } from "effect";
import type * as Atom from "effect/reactivity/Atom";
import {
  type FunctionComponent,
  type ReactNode,
  createElement,
  memo,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useReducer,
  useSyncExternalStore,
} from "react";

import { isPending, runReading } from "../../atoms.ts";
import { Failure } from "../../cause.ts";
import { BridgeContext, RegistryContext, toReact } from "./render.ts";
import { type Binding, type DomEvent, type Ui, inputSnapshot, uiComponent } from "./nodes.ts";
import type { Handler } from "./policies.ts";

/**
 * `View p r e`: a component whose render reads atoms through `Atom.Read`.
 * React renders it again only when an atom it read changes, or its props.
 */
export interface View {
  readonly render: (props: unknown) => Effect.Effect<Ui, unknown, unknown>;
  /** The name React's tools show, `Module.name`. */
  readonly label: string | undefined;
}

/** `Ui.view`: a component of `render`. */
export const uiView = (render: View["render"]): View => Object.freeze({ render, label: undefined });

/**
 * `Ui.suspended`: a view whose render may read a loading task (it shows
 * `fallback`) or a failed one (it shows `failed` of the failure).
 */
export const uiSuspended = (failed: (failure: Failure) => Ui, fallback: Ui, render: View["render"]): View =>
  uiView((props) =>
    Effect.catchCause(render(props), (cause) => {
      if (isPending(cause)) return Effect.succeed(fallback);
      if (Cause.hasFails(cause)) return Effect.succeed(failed(new Failure(cause)));
      return Effect.failCause(cause);
    }),
  );

/** `Ui.named`: the component as React's tools show it. */
export const uiNamed = (label: string, view: View): View => Object.freeze({ render: view.render, label });

const registryOf = (what: string) => {
  const registry = useContext(RegistryContext);
  if (registry === undefined) throw new Error(`${what}: render it with \`Ui.Dom.mount\`, which provides the registry`);
  return registry;
};

/** One React component per view, named as React's tools show it. */
const components = new WeakMap<View, FunctionComponent<object>>();

const component = (view: View): FunctionComponent<object> => {
  const known = components.get(view);
  if (known !== undefined) return known;
  const render = ({ props }: { readonly props: unknown }): ReactNode => {
    const registry = registryOf("a view reads atoms");
    const bridge = useContext(BridgeContext);
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
  const result = made as unknown as FunctionComponent<object>;
  components.set(view, result);
  return result;
};

/** `Ui.render`: the component with these props. */
export const uiRender = (view: View, props: unknown): Ui => uiComponent(component(view), { props });

interface FieldState {
  readonly tag: "input" | "textarea";
  readonly bind: Atom.Writable<string>;
  readonly placeholder: string;
  readonly input: Binding | undefined;
}

/**
 * A field bound to a state: the value comes from the registry and each
 * change goes to it synchronously, inside the DOM callback, so React keeps
 * the caret and an IME composition intact; `on_input` then runs as a task.
 */
const Field = ({ tag, bind, placeholder, input }: FieldState): ReactNode => {
  const registry = registryOf("a field binds a state");
  const bridge = useContext(BridgeContext);
  const subscribe = useCallback((changed: () => void) => registry.subscribe(bind, changed), [registry, bind]);
  const value = useSyncExternalStore(subscribe, () => registry.get(bind));
  const listener = useMemo(() => (bridge === undefined || input === undefined ? undefined : bridge.listener(input)), [bridge]);
  useLayoutEffect(() => {
    if (listener !== undefined && input !== undefined) listener.update(input);
  });
  useLayoutEffect(() => () => listener?.close(), [listener]);
  const onChange = (event: { readonly target: { readonly value: string } } & DomEvent): void => {
    registry.set(bind, event.target.value);
    listener?.callback(event);
  };
  return createElement(tag, { value, placeholder, onChange, ...(tag === "textarea" ? { spellCheck: false } : {}) });
};

interface FieldProps {
  readonly bind: Atom.Writable<string>;
  readonly placeholder: string;
  readonly on_input: Option.Option<Handler>;
}

const field =
  (tag: "input" | "textarea") =>
  (props: FieldProps): Ui => {
    const input = Option.isSome(props.on_input) ? { handler: props.on_input.value, snapshot: inputSnapshot } : undefined;
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the field's props are its state
    return uiComponent(Field as FunctionComponent<object>, { tag, bind: props.bind, placeholder: props.placeholder, input });
  };

/** `Ui.field`: an input showing `bind` and writing each keystroke to it at once. */
export const uiField = /*#__PURE__*/ field("input");

/** `Ui.field_area`: a text area bound as `Ui.field` is. */
export const uiFieldArea = /*#__PURE__*/ field("textarea");
