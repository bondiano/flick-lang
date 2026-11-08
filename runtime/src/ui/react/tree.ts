// A `Ui` as React elements: elements with event props take their listeners
// from the root's bridge, and components (foreign ones, views, fields)
// render as React components.
import { Fragment, type ReactNode, createElement, useLayoutEffect, useMemo } from "react";

import type { Listener } from "./handlers.ts";
import type { Binding, Ui } from "./nodes.ts";

/** What elements with event props need of a root's bridge. */
export interface Listeners {
  listener(binding: Binding): Listener;
}

type ElementNode = Extract<Ui, { readonly kind: "element" }>;

const EventElement = ({ bridge, node }: { readonly bridge: Listeners; readonly node: ElementNode }): ReactNode => {
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
  if (node.handle !== undefined) props["ref"] = node.handle.ref;
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


export const toReact = (bridge: Listeners | undefined, node: Ui): ReactNode => {
  if (node.kind === "text") return node.value;
  if (node.kind === "fragment") return createElement(Fragment, null, ...children(bridge, node.children));
  if (node.kind === "keyed") return createElement(Fragment, { key: node.key }, toReact(bridge, node.node));
  if (node.kind === "component") return createElement(node.component, node.props);
  if (bridge !== undefined && Object.keys(node.events).length > 0) return createElement(EventElement, { bridge, node });
  const props: Record<string, unknown> = { ...node.attrs };
  if (node.handle !== undefined) props["ref"] = node.handle.ref;
  if (node.html === undefined) return createElement(node.tag, props, ...children(bridge, node.children));
  props["dangerouslySetInnerHTML"] = { __html: node.html };
  return createElement(node.tag, props);
};

const children = (bridge: Listeners | undefined, nodes: ReadonlyArray<Ui>): Array<ReactNode> => nodes.map((node) => toReact(bridge, node));

export interface RenderRoot {
  render(node: ReactNode): void;
  unmount(): void;
}

/** A backend opens its root when the effect runs, not when the description is built. */
export type Renderer = () => RenderRoot;
