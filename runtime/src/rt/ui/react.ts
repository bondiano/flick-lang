// `@flick-lang/runtime/ui/react` on Flick's own runtime: the elements of `Ui`,
// which need no runtime, and roots whose events run as fibers of the kernel.
// When the fiber running a root is interrupted, events stop, React unmounts,
// and the tasks are interrupted and joined.
import { Component, type ReactNode, createElement } from "react";

import { type Option, none, some } from "../../core/data.ts";
import type { ElementHandle, Ui } from "../../ui/react/nodes.ts";
import { isSafeUrl, plainClick, routeLink } from "../../ui/react/nodes.ts";
import { eventLatest } from "../../ui/react/handlers.ts";
import type { RenderRoot, Renderer } from "../../ui/react/tree.ts";
import { toReact } from "../../ui/react/tree.ts";
import type { Atom, Registry } from "../atoms/registry.ts";
import { seedStates, statesOf } from "../atoms/atom.ts";
import { WRITE, registryHere } from "../atoms/services.ts";
import { hotHooks } from "../hot.ts";
import { type UrlCodec, routePrint } from "../route.ts";
import { type Cause, Raise, causeOf, fail, raise } from "../cause.ts";
import { type Run, Suspend, asExit, atMask, awaitFiber, currentCtx, currentFiber, exitOf, extend, raiseOf } from "../fiber.ts";
import { service } from "../handle.ts";
import type { Failure } from "../outcome.ts";
import { finalize } from "../owner.ts";
import { tag } from "../../tags.ts";
import { Bridge } from "./bridge.ts";
import { BridgeContext, RegistryContext } from "./render.ts";

export * from "../../ui/react/nodes.ts";
export {
  actionConcurrent,
  actionExhaust,
  actionLatest,
  actionQueue,
  actionWith,
  eventConcurrent,
  eventExhaust,
  eventLatest,
  eventPreventDefault,
  eventQueue,
} from "../../ui/react/handlers.ts";
export { uiRenderStatic } from "./render.ts";
export * from "./views.ts";

/** `Ui.safe_url`: a relative URL, or one with a safe scheme. */
export const uiSafeUrl = (url: string): Option<string> => (isSafeUrl(url) ? some(url) : none);

interface RouteLinkProps {
  readonly route: Atom;
  readonly url: UrlCodec;
  readonly to: Readonly<Record<string, unknown>>;
}

/**
 * A link inside the app: a plain click writes `to` to the route state, which
 * pushes its URL without a reload; another click, and markup without a
 * client, follow the real `href`.
 */
export const uiRouteLink = (props: RouteLinkProps, children: ReadonlyArray<Ui>): Ui => {
  // a handler's task runs with the root's `Atom.Write`, as one of Flick's does
  const navigate = (): undefined => {
    registryHere(WRITE).set(props.route, props.to);
    return undefined;
  };
  const handler = eventLatest((snapshot) => (plainClick(snapshot) ? navigate : () => undefined));
  return routeLink(routePrint(props.url, props.to), handler, children);
};

const DETACHED = /*#__PURE__*/ tag("Ui.Detached.Detached");

interface Node {
  readonly isConnected: boolean;
  focus(): void;
  blur(): void;
  scrollIntoView(): void;
  getBoundingClientRect(): { readonly width: number; readonly height: number; readonly top: number; readonly left: number };
}

const isNode = (node: unknown): node is Node =>
  typeof node === "object" && node !== null && Reflect.get(node, "isConnected") === true && typeof Reflect.get(node, "focus") === "function";

/** What `f` does with the mounted node of `handle`; a handle bound to none fails with `Ui.Detached`. */
const onNode =
  <A>(f: (node: Node) => A) =>
  (handle: ElementHandle): A =>
    isNode(handle.node) ? f(handle.node) : raise(fail({ _tag: DETACHED }));

/** `Ui.Document`, which a root gives the tasks of its handlers and instances. */
const DOCUMENT = /*#__PURE__*/ service("Ui.Document");
const document = {
  focus: onNode((node) => node.focus()),
  blur: onNode((node) => node.blur()),
  scroll_into_view: onNode((node) => node.scrollIntoView()),
  measure: onNode((node) => {
    const { width, height, top, left } = node.getBoundingClientRect();
    return { width, height, top, left };
  }),
};

class RenderBoundary extends Component<{ readonly broken: (cause: Cause) => void; readonly children: ReactNode }, { readonly failed: boolean }> {
  override state = { failed: false };

  static getDerivedStateFromError(): { readonly failed: boolean } {
    return { failed: true };
  }

  override componentDidCatch(error: unknown): void {
    // unmount only after React has finished this commit
    queueMicrotask(() => this.props.broken(causeOf(error)));
  }

  override render(): ReactNode {
    return this.state.failed ? null : this.props.children;
  }
}

/** The registries of the mounted roots, which a module replacement in development reads. */
const mounted = new Set<Registry>();

/** The states the next root starts from, kept across a module replacement. */
let carried: ReadonlyMap<string, unknown> | undefined;

hotHooks(
  () => {
    const states = new Map<string, unknown>();
    for (const registry of mounted) for (const [name, value] of statesOf(registry)) states.set(name, value);
    return states;
  },
  (states) => {
    carried = states;
  },
);

/**
 * Renders `ui` through a React renderer, once: its views render again by
 * themselves when the atoms they read from `registry` change. Events run as
 * tasks of the root under their policies.
 */
export function* uiRunWith(renderer: Renderer, supervisor: (failure: Failure) => unknown, ui: Ui, registry: Registry): Run {
  const fiber = currentFiber();
  const depth = fiber.mask;
  // the body is interruptible, as the caller was; the teardown is not
  fiber.mask = depth + 1;
  try {
    let failure: Cause | undefined;
    let resume: ((cause: Cause) => void) | undefined;
    const broken = (cause: Cause): void => {
      if (failure !== undefined) return;
      failure = cause;
      resume?.(cause);
    };
    // its handlers and instances reach the nodes of its elements
    const bridge = new Bridge(extend(currentCtx(), DOCUMENT, document), supervisor, broken);
    let root: RenderRoot | undefined;
    const ran = asExit(
      yield exitOf(() =>
        atMask(depth, () => {
          root = renderer();
          if (carried !== undefined) seedStates(registry, carried);
          carried = undefined;
          mounted.add(registry);
          try {
            const tree = createElement(BridgeContext.Provider, {
              value: bridge,
              children: createElement(RenderBoundary, { broken, children: toReact(bridge, ui) }),
            });
            root.render(createElement(RegistryContext.Provider, { value: registry }, tree));
          } catch (error) {
            broken(causeOf(error));
          }
          return new Suspend((_resolve, reject) => {
            resume = (cause) => reject(new Raise(cause));
            if (failure !== undefined) resume(failure);
          });
        }),
      ),
    );
    bridge.accepting = false;
    const unmounted = asExit(yield exitOf(() => root?.unmount()));
    mounted.delete(registry);
    const tasks = [...bridge.tasks];
    for (const task of tasks) task.interrupt(fiber.id);
    for (const task of tasks) yield awaitFiber(task);
    for (const supervising of Array.from(bridge.supervisors)) yield awaitFiber(supervising);
    return raiseOf(finalize(ran, unmounted));
  } finally {
    fiber.mask = depth;
  }
}

/** `Ui.mount_with`: `uiRunWith` with the registry of the provided `Reactivity.Atom.Registry`. */
export const uiMountWith = (renderer: Renderer, supervisor: (failure: Failure) => unknown, ui: Ui): Run =>
  uiRunWith(renderer, supervisor, ui, registryHere());
