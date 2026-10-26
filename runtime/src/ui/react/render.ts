// A `Ui` as React elements: elements with event props take their listeners
// from the root's bridge, and components (foreign ones, views, fields)
// render as React components.
import type * as Atom from "../../effect/reactivity/Atom.ts";
import type * as AtomRegistry from "../../effect/reactivity/AtomRegistry.ts";
import { Fragment, type ReactNode, createContext, createElement, useLayoutEffect, useMemo } from "react";

import type { Ui } from "./nodes.ts";
import type { Bridge, Listener } from "./policies.ts";

export type Events = Pick<Bridge<never>, "listener" | "release" | "fork">;
type ElementNode = Extract<Ui, { readonly kind: "element" }>;

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

/** The registry views read atoms from: `Ui.Dom.mount` provides it. */
export const RegistryContext = /*#__PURE__*/ createContext<AtomRegistry.AtomRegistry | undefined>(undefined);

/** How a server streaming markup waits for a change of the atoms a suspended view read, with the values it read. */
export interface Streaming {
  readonly wait: (read: ReadonlyMap<Atom.Atom<unknown>, unknown>) => PromiseLike<void>;
}

/** Given by `Ui.Server.stream`: a suspended view then suspends while a task loads, and its markup follows later. */
export const StreamContext = /*#__PURE__*/ createContext<Streaming | undefined>(undefined);

/** The events of the root, for components that render a `Ui` of their own. */
export const BridgeContext = /*#__PURE__*/ createContext<Events | undefined>(undefined);

export const toReact = (bridge: Events | undefined, node: Ui): ReactNode => {
  if (node.kind === "text") return node.value;
  if (node.kind === "fragment") return createElement(Fragment, null, ...children(bridge, node.children));
  if (node.kind === "keyed") return createElement(Fragment, { key: node.key }, toReact(bridge, node.node));
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

