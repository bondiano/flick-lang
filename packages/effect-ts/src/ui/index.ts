// `Ui.Dom.mount` on Effect: a root in a DOM container, with the atoms a server
// wrote into the document.
import { Effect } from "effect";
import type * as AtomRegistry from "../effect/reactivity/AtomRegistry.ts";
import * as Hydration from "../effect/reactivity/Hydration.ts";

import type { Failure } from "../cause.ts";
import { RegistryService, nativeRegistry } from "../effect/reactivity/Atom.ts";
import { domRenderer } from "@flick-lang/runtime/ui/dom";
import { type Ui, uiMountWith } from "./react/index.ts";

export * from "./react/index.ts";
export { uiContainer } from "@flick-lang/runtime/ui/dom";

/**
 * The atoms the server wrote into the document for `container`, by
 * `Ui.Server.state` or in the scripts of `Ui.Server.stream`, into
 * `registry`. Each script is removed, so a later root does not write its
 * atoms over values changed since.
 */
const hydrateState = (container: Element, registry: AtomRegistry.AtomRegistry): void => {
  const document = container.ownerDocument;
  const streamed = Array.from(document.querySelectorAll("script[data-flick-state]")).filter(
    (script) => script.getAttribute("data-flick-state") === container.id,
  );
  for (const script of [document.getElementById(`flick-state-${container.id}`), ...streamed]) {
    if (script === null) continue;
    script.remove();
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Ui.Server.state` and `Ui.Server.stream` wrote these entries
    Hydration.hydrate(registry, JSON.parse(script.textContent ?? "[]") as Array<Hydration.DehydratedAtom>);
  }
};

/**
 * Hydrates the atoms of each script a streamed page adds to `container`
 * after the root mounted, until the root unmounts: a script comes before
 * the markup it holds the atoms of, and a mutation observer runs before
 * React hydrates that markup.
 */
const hydrateStreamed = (container: Element, registry: AtomRegistry.AtomRegistry): (() => void) => {
  const Observer = container.ownerDocument.defaultView?.MutationObserver;
  if (Observer === undefined || !container.hasAttribute("data-flick-ssr")) return () => undefined;
  const observer = new Observer(() => hydrateState(container, registry));
  // the stream's later chunks follow the container, which it rendered itself
  observer.observe(container.parentNode ?? container.ownerDocument, { childList: true });
  return () => observer.disconnect();
};

/**
 * `Ui.Dom.mount`: renders `ui` into `container` with the registry of the
 * provided `Reactivity.Atom.Registry`, hydrating markup a server rendered
 * and, before the first render, the atoms of `Ui.Server.state`.
 */
export const uiMount = <R>(
  container: Element,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  ui: Ui,
): Effect.Effect<never, unknown, unknown> =>
  Effect.flatMap(Effect.service(RegistryService), (record) => {
    const registry = nativeRegistry(record);
    const mount = uiMountWith(domRenderer(container), supervisor, ui);
    if (registry === undefined) return mount;
    return Effect.acquireUseRelease(
      Effect.sync(() => {
        hydrateState(container, registry);
        return hydrateStreamed(container, registry);
      }),
      () => mount,
      (stop) => Effect.sync(stop),
    );
  });
