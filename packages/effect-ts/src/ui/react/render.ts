// The registry views read atoms from, through React context, and the static
// render of server markup.
import type * as Atom from "../../effect/reactivity/Atom.ts";
import * as AtomRegistry from "../../effect/reactivity/AtomRegistry.ts";
import { type ReactNode, createContext, createElement, useContext, useMemo } from "react";

import type { Ui } from "@flick-lang/runtime/ui/react/nodes";
import type { Bridge } from "./policies.ts";
import { toReact } from "@flick-lang/runtime/ui/react/tree";

export * from "@flick-lang/runtime/ui/react/tree";

export type Events = Pick<Bridge<never>, "listener" | "release" | "fork">;

/** The events of the root, for components that render a `Ui` of their own. */
export const BridgeContext = /*#__PURE__*/ createContext<Events | undefined>(undefined);

/** The registry views read atoms from: `Ui.Dom.mount` provides it. */
export const RegistryContext = /*#__PURE__*/ createContext<AtomRegistry.AtomRegistry | undefined>(undefined);

/** How a server streaming markup waits for a change of the atoms a suspended view read, with the values it read. */
export interface Streaming {
  readonly wait: (read: ReadonlyMap<Atom.Atom<unknown>, unknown>) => PromiseLike<void>;
}

/** Given by `Ui.Server.stream`: a suspended view then suspends while a task loads, and its markup follows later. */
export const StreamContext = /*#__PURE__*/ createContext<Streaming | undefined>(undefined);

/** A registry of its own for a static render when no provider is above it. */
const WithRegistry = ({ node }: { readonly node: Ui }): ReactNode => {
  const given = useContext(RegistryContext);
  // like `Ui.Server.to_html`: a registry of this call alone, so the same `ui` gives the same markup
  const registry = useMemo(() => given ?? AtomRegistry.make(), [given]);
  return createElement(RegistryContext.Provider, { value: registry }, toReact(undefined, node));
};

/**
 * The same inert view for server markup; event bindings are attached by the
 * client root. Its views and fields read atoms as a server does, from a
 * registry of this call alone, unless a provider above gives one, as
 * `Ui.Server.render` and `Ui.Server.stream` do with a request's registry.
 */
export const uiRenderStatic = (node: Ui): ReactNode => createElement(WithRegistry, { node });
