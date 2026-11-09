// What views on Flick's own runtime take from React context: the registry
// they read atoms from and the events of their root; and the static render
// of server markup.
import { type ReactNode, createContext, createElement, useContext, useMemo } from "react";

import type { Handler } from "../../ui/react/handlers.ts";
import type { Ui } from "../../ui/react/nodes.ts";
import { type Listeners, toReact } from "../../ui/react/tree.ts";
import { Registry } from "../atoms/registry.ts";
import { type Fiber, currentCtx } from "../fiber.ts";

/** What views and fields need of a root's bridge. */
export interface Events extends Listeners {
  release(actions: ReadonlyArray<Handler>): void;
  fork(task: () => unknown): Fiber;
}

/** The registry views read atoms from: `Ui.Dom.mount` provides it. */
export const RegistryContext = /*#__PURE__*/ createContext<Registry | undefined>(undefined);

/** The events of the root, for components that render a `Ui` of their own. */
export const BridgeContext = /*#__PURE__*/ createContext<Events | undefined>(undefined);

/** A registry of its own for a static render when no provider is above it. */
const WithRegistry = ({ node }: { readonly node: Ui }): ReactNode => {
  const given = useContext(RegistryContext);
  // like `Ui.Server.to_html`: a registry of this call alone, so the same `ui` gives the same markup
  const registry = useMemo(() => given ?? new Registry(currentCtx()), [given]);
  return createElement(RegistryContext.Provider, { value: registry }, toReact(undefined, node));
};

/**
 * The same inert view for server markup; event bindings are attached by the
 * client root. Its views and fields read atoms as a server does, from the
 * registry a provider above gives, else one of this call alone.
 */
export const uiRenderStatic = (node: Ui): ReactNode => createElement(WithRegistry, { node });
