// Views and fields on atoms on Flick's own runtime: React components a `Ui`
// holds as foreign components do. A render reads atoms through `Atom.Read`
// at once; a setup makes an instance's states through `Setup`.
import {
  type FunctionComponent,
  type ReactNode,
  Suspense,
  createElement,
  memo,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useSyncExternalStore,
} from "react";

import type { Option } from "../../core/data.ts";
import { type Handler, instanceActions } from "../../ui/react/handlers.ts";
import { type Attrs, type Binding, type DomEvent, type Ui, element, inputSnapshot, uiComponent, uiFragment } from "../../ui/react/nodes.ts";
import { toReact } from "../../ui/react/tree.ts";
import { atomDerived } from "../atoms/atom.ts";
import { type Atom, type Registry, isAtom } from "../atoms/registry.ts";
import { isPending, reading } from "../atoms/services.ts";
import { causeOf, hasFails } from "../cause.ts";
import { type Fiber, type Run, awaitFiber, currentCtx, extend, inCtx, runIn } from "../fiber.ts";
import { service } from "../handle.ts";
import { Failure } from "../outcome.ts";
import { BridgeContext, type Events, RegistryContext, StreamContext } from "./render.ts";

/** `View p r e`: a component whose render reads atoms; React renders it again when one it read changes, or its props. */
export interface View {
  /** Sync: it may fail only under `Ui.suspended`. */
  readonly render: (props: unknown) => Ui;
  /** The name React's tools show, `Module.name`. */
  readonly label: string | undefined;
  /** What a view of `Ui.suspended` shows while a task it read loads. */
  readonly fallback?: Ui;
  /** `Ui.stateful`: makes the render of an instance, once per instance. */
  readonly setup?: (props: unknown) => (props: unknown) => Ui;
}

/** `Lifecycle`: work of an instance from its mount to its unmount. */
type Lifecycle =
  | { readonly kind: "mount"; readonly task: () => unknown }
  | { readonly kind: "watch"; readonly read: (unit: undefined) => unknown; readonly run: (value: unknown) => () => unknown };

/** `Ui.mounted`. */
export const uiMounted = (task: () => unknown): Lifecycle => ({ kind: "mount", task });

/** `Ui.watched`. */
export const uiWatched = (read: (unit: undefined) => unknown, run: (value: unknown) => () => unknown): Lifecycle => ({
  kind: "watch",
  read,
  run,
});

const SETUP = /*#__PURE__*/ service("Prelude.Setup");

/** What an instance renders with: its view, the actions its setup made, its work, and the atoms it owns. */
interface Instance {
  readonly view: View;
  readonly actions: ReadonlyArray<Handler>;
  readonly work: ReadonlyArray<Lifecycle>;
  readonly atoms: ReadonlyArray<Atom>;
}

/** The instance `setup` makes with these first props. */
const setUp = (setup: (props: unknown) => (props: unknown) => Ui, props: unknown, view: View): Instance => {
  const work: Array<Lifecycle> = [];
  const atoms: Array<Atom> = [];
  const impl = {
    own: (value: unknown): unknown => {
      if (isAtom(value)) atoms.push(value);
      return value;
    },
    start: (w: Lifecycle): undefined => void work.push(w),
  };
  const [render, actions] = instanceActions(() =>
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a setup gives the render of its instance
    inCtx(extend(currentCtx(), SETUP, impl), () => runIn(setup(props)) as (props: unknown) => Ui),
  );
  return { view: { ...view, render }, actions, work, atoms };
};

const instanceOf = (view: View, props: unknown): Instance =>
  view.setup === undefined ? { view, actions: [], work: [], atoms: [] } : setUp(view.setup, props, view);

const stop = (fiber: Fiber | undefined): void => fiber?.interrupt(-1);

/** `task` once `previous` has been interrupted and has ended. */
function* after(previous: Fiber | undefined, task: () => unknown): Run {
  if (previous !== undefined) {
    stop(previous);
    yield awaitFiber(previous);
  }
  return task();
}

/** Starts `work` as tasks of the root; the result stops them. */
const begin = (bridge: Events, registry: Registry, work: Lifecycle): (() => void) => {
  if (work.kind === "mount") {
    const fiber = bridge.fork(work.task);
    return () => stop(fiber);
  }
  let current: Fiber | undefined;
  const unsubscribe = registry.subscribe(
    atomDerived(work.read),
    (value) => {
      const previous = current;
      current = bridge.fork(() => after(previous, work.run(value)));
    },
    true,
  );
  return () => {
    unsubscribe();
    stop(current);
  };
};

/** `Ui.stateful`: instances render with what `setup` made for them; without one, as on a server, it sets up on each render. */
export const uiStateful = (setup: (props: unknown) => (props: unknown) => Ui): View =>
  Object.freeze({
    render: (props: unknown) => setUp(setup, props, { render: () => uiFragment([]), label: undefined }).view.render(props),
    setup,
    label: undefined,
  });

/** `Ui.view`: a component of the Sync `render`. */
export const uiView = (render: (props: unknown) => Ui): View => Object.freeze({ render, label: undefined });

/** `Ui.suspended`: a view whose render may read a loading task (it shows `fallback`) or a failed one (`failed` of the failure). */
export const uiSuspended = (failed: (failure: Failure) => Ui, fallback: Ui, render: (props: unknown) => Ui): View =>
  Object.freeze({
    render: (props: unknown): Ui => {
      try {
        // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the render gives a `Ui`
        return runIn(render(props)) as Ui;
      } catch (thrown) {
        const cause = causeOf(thrown);
        if (!isPending(cause) && hasFails(cause)) return failed(new Failure(cause));
        throw thrown;
      }
    },
    label: undefined,
    fallback,
  });

/** `Ui.named`: the component as React's tools show it. */
export const uiNamed = (label: string, view: View): View => Object.freeze({ ...view, label });

const STILL_LOADING = "`Atom.await` read a task still loading outside `Ui.suspended`, which shows a fallback meanwhile";

/** The node `view` renders; while a task it read loads, `pending()`. */
const draw = (view: View, props: unknown, read: (atom: Atom) => unknown, pending: () => Ui): Ui => {
  try {
    return reading(read, () => view.render(props));
  } catch (thrown) {
    if (!isPending(causeOf(thrown))) throw thrown;
    if (view.fallback !== undefined) return pending();
    throw new Error(STILL_LOADING, { cause: thrown });
  }
};

const unsubscribed = (): (() => void) => () => undefined;
const onClient = (): boolean => false;
const onServer = (): boolean => true;

/** Whether the component renders on a server or hydrates server markup: it then reads the server values of atoms. */
const serverSide = (): boolean => useSyncExternalStore(unsubscribed, onClient, onServer);

const registryOf = (what: string): Registry => {
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
    // set up once per instance: StrictMode calls a state's initializer twice, not a ref's
    const ref = useRef<Instance | undefined>(undefined);
    ref.current ??= instanceOf(view, props);
    const { view: own, actions, work, atoms } = ref.current;
    useLayoutEffect(() => {
      // the registry forgets the instance's atoms once it unmounts
      const held = atoms.map((atom) => registry.mount(atom));
      const stops = bridge === undefined ? [] : work.map((w) => begin(bridge, registry, w));
      return () => {
        for (const s of stops) s();
        bridge?.release(actions);
        for (const release of held) release();
      };
    }, [bridge, registry, actions, work, atoms]);
    const read = new Map<Atom, unknown>();
    const node = draw(
      own,
      props,
      (atom) => {
        const value = server ? registry.serverValue(atom) : registry.get(atom);
        read.set(atom, value);
        return value;
      },
      () => {
        // a streamed page shows the boundary's fallback and sends this view once a read atom changes
        // oxlint-disable-next-line typescript/only-throw-error -- React suspends a render that throws a promise
        if (streaming !== undefined) throw streaming.wait(read);
        return view.fallback ?? uiFragment([]);
      },
    );
    // subscribe after the commit; a write since the render renders again
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
  readonly bind: Atom<string>;
  readonly placeholder: string;
  readonly input: Binding | undefined;
  /** Host attributes of the element, from `Ui.with_host`. */
  readonly attrs: Attrs;
}

/**
 * A field bound to a state: the value comes from the registry and each
 * change goes to it synchronously, inside the DOM callback, so React keeps
 * the caret and an IME composition intact; `on_input` then runs as a task.
 */
const Field = ({ tag, bind, placeholder, input, attrs }: FieldState): ReactNode => {
  const registry = registryOf("a field binds a state");
  const bridge = useContext(BridgeContext);
  const subscribe = useCallback((changed: () => void) => registry.subscribe(bind, changed), [registry, bind]);
  const value = useSyncExternalStore(
    subscribe,
    () => registry.get(bind),
    () => String(registry.serverValue(bind)),
  );
  const hasInput = input !== undefined;
  const listener = useMemo(() => (bridge === undefined || input === undefined ? undefined : bridge.listener(input)), [bridge, hasInput]);
  useLayoutEffect(() => {
    if (listener !== undefined && input !== undefined) listener.update(input);
  });
  useLayoutEffect(() => () => listener?.close(), [listener]);
  const onChange = (event: { readonly target: { readonly value: string } } & DomEvent): void => {
    registry.set(bind, event.target.value);
    listener?.callback(event);
  };
  return createElement(tag, { ...attrs, value, placeholder, onChange, ...(tag === "textarea" ? { spellCheck: false } : {}) });
};

interface FieldProps {
  readonly bind: Atom<string>;
  readonly placeholder: string;
  readonly on_input: Option<Handler>;
}

const fieldWith = (tag: "input" | "textarea", props: FieldProps, attrs: Attrs): Ui => {
  const input = props.on_input._tag === "Some" ? { handler: props.on_input.value, snapshot: inputSnapshot } : undefined;
  const state: FieldState = { tag, bind: props.bind, placeholder: props.placeholder, input, attrs };
  return Object.freeze({
    kind: "component",
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the field's props are its state
    component: Field as FunctionComponent<object>,
    props: state,
    static: (read: (atom: object) => unknown) =>
      element(tag, { ...attrs, placeholder: props.placeholder, value: String(read(props.bind)) }, {}, []),
    hosted: (add: (attrs: Attrs) => Attrs) => fieldWith(tag, props, add(attrs)),
  });
};

/** `Ui.field`: an input showing `bind` and writing each keystroke to it at once. */
export const uiField = (props: FieldProps): Ui => fieldWith("input", props, {});

/** `Ui.field_area`: a text area bound as `Ui.field` is. */
export const uiFieldArea = (props: FieldProps): Ui => fieldWith("textarea", props, {});
