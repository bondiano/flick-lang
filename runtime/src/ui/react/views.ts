// Views and fields on atoms: React components a `Ui` holds as foreign
// components do, so a program that renders none keeps none of this code.
// They take the root's events and registry from React context.
import { Cause, Effect, Option } from "effect";
import * as Atom from "effect/reactivity/Atom";
import {
  type FunctionComponent,
  type ReactNode,
  createElement,
  Suspense,
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
import { BridgeContext, RegistryContext, StreamContext, toReact } from "./render.ts";
import { type Binding, type DomEvent, type Static, type Ui, element, inputSnapshot, uiComponent, uiFragment } from "./nodes.ts";
import type { Handler } from "./policies.ts";

/**
 * `View p r e`: a component whose render reads atoms through `Atom.Read`.
 * React renders it again only when an atom it read changes, or its props.
 */
export interface View {
  readonly render: (props: unknown) => Effect.Effect<Ui, unknown, unknown>;
  /** The name React's tools show, `Module.name`. */
  readonly label: string | undefined;
  /** What a view of `Ui.suspended` shows while a task it read loads. */
  readonly fallback?: Ui;
}

/** `Ui.view`: a component of `render`. */
export const uiView = (render: View["render"]): View => Object.freeze({ render, label: undefined });

/**
 * `Ui.suspended`: a view whose render may read a loading task (it shows
 * `fallback`) or a failed one (it shows `failed` of the failure).
 */
export const uiSuspended = (failed: (failure: Failure) => Ui, fallback: Ui, render: View["render"]): View =>
  Object.freeze({
    render: (props: unknown) =>
      Effect.catchCause(render(props), (cause) => {
        if (!isPending(cause) && Cause.hasFails(cause)) return Effect.succeed(failed(new Failure(cause)));
        return Effect.failCause(cause);
      }),
    label: undefined,
    fallback,
  });

/** `Ui.named`: the component as React's tools show it. */
export const uiNamed = (label: string, view: View): View => Object.freeze({ ...view, label });

/** The node `view` renders; while a task it read loads, `pending()`. */
const draw = (view: View, props: unknown, read: (atom: Atom.Atom<unknown>) => unknown, pending: () => Ui): Ui =>
  runReading(view.render(props), read, view.fallback === undefined ? undefined : pending);

const unsubscribed = (): (() => void) => () => undefined;
const onClient = (): boolean => false;
const onServer = (): boolean => true;

/**
 * Whether the component renders on a server or hydrates server markup:
 * it then reads the server values of atoms, as the server did, and React
 * renders it again with the client's once hydrated.
 */
const serverSide = (): boolean => useSyncExternalStore(unsubscribed, onClient, onServer);

const registryOf = (what: string) => {
  const registry = useContext(RegistryContext);
  if (registry === undefined) throw new Error(`${what}: render it with \`Ui.Dom.mount\` or \`Ui.Server.render\`, which provide the registry`);
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
    const server = serverSide();
    const streaming = useContext(StreamContext);
    const read = new Map<Atom.Atom<unknown>, unknown>();
    const reading = (atom: Atom.Atom<unknown>): unknown => {
      const value = server ? Atom.getServerValue(atom, registry) : registry.get(atom);
      read.set(atom, value);
      return value;
    };
    const node = draw(view, props, reading, () => {
      // a streamed page shows the boundary's fallback and sends this view once a read atom changes
      // oxlint-disable-next-line typescript/only-throw-error -- React suspends a render that throws a promise
      if (streaming !== undefined) throw streaming.wait([...read.keys()]);
      return view.fallback ?? uiFragment([]);
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
  const fallback = view.fallback;
  // a suspended view is a boundary on server and client alike, so streamed markup hydrates
  const bounded =
    fallback === undefined
      ? made
      : memo(({ props }: { readonly props: unknown }): ReactNode =>
          createElement(Suspense, { fallback: toReact(useContext(BridgeContext), fallback) }, createElement(made, { props })),
        );
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `memo` of a function component is one to render
  const result = bounded as unknown as FunctionComponent<object>;
  components.set(view, result);
  return result;
};

/** `Ui.render`: the component with these props. */
export const uiRender = (view: View, props: unknown): Ui =>
  uiComponent(component(view), { props }, (read) => draw(view, props, read, () => view.fallback ?? uiFragment([])));

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
  const snapshot = (): string => registry.get(bind);
  const value = useSyncExternalStore(subscribe, snapshot, () => Atom.getServerValue(bind, registry));
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
    const toUi: Static = (read) => element(tag, { placeholder: props.placeholder, value: String(read(props.bind)) }, {}, []);
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the field's props are its state
    return uiComponent(Field as FunctionComponent<object>, { tag, bind: props.bind, placeholder: props.placeholder, input }, toUi);
  };

/** `Ui.field`: an input showing `bind` and writing each keystroke to it at once. */
export const uiField = /*#__PURE__*/ field("input");

/** `Ui.field_area`: a text area bound as `Ui.field` is. */
export const uiFieldArea = /*#__PURE__*/ field("textarea");
