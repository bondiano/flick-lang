// `@flick-lang/runtime/ui` on Flick's own runtime: a root in a DOM container,
// with the atoms a server wrote into the document.
import type { Ui } from "../ui/react/nodes.ts";
import { domRenderer } from "../ui/dom.ts";
import type { Registry } from "./atoms/registry.ts";
import { hydrate } from "./atoms/serial.ts";
import { registryHere } from "./atoms/services.ts";
import type { Run } from "./fiber.ts";
import type { Failure } from "./outcome.ts";
import { uiRunWith } from "./ui/react.ts";

export * from "./ui/react.ts";
export { uiContainer } from "../ui/dom.ts";

/**
 * The atoms the server wrote into the document for `container`, by
 * `Ui.Server.state` or in the scripts of `Ui.Server.stream`, into
 * `registry`. Each script is removed, so a later root does not write its
 * atoms over values changed since.
 */
const hydrateState = (container: Element, registry: Registry): void => {
  const document = container.ownerDocument;
  const streamed = Array.from(document.querySelectorAll("script[data-flick-state]")).filter(
    (script) => script.getAttribute("data-flick-state") === container.id,
  );
  for (const script of [document.getElementById(`flick-state-${container.id}`), ...streamed]) {
    if (script === null) continue;
    script.remove();
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Ui.Server.state` and `Ui.Server.stream` wrote these entries
    hydrate(registry, JSON.parse(script.textContent ?? "[]") as Array<{ key: string; value: unknown }>);
  }
};

/**
 * Hydrates the atoms of each script a streamed page adds after the root
 * mounted, until the root unmounts: a script comes before the markup it
 * holds the atoms of, and a mutation observer runs before React hydrates
 * that markup.
 */
const hydrateStreamed = (container: Element, registry: Registry): (() => void) => {
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
export function* uiMount(container: Element, supervisor: (failure: Failure) => unknown, ui: Ui): Run {
  const registry = registryHere();
  hydrateState(container, registry);
  const stop = hydrateStreamed(container, registry);
  try {
    return yield* uiRunWith(domRenderer(container), supervisor, ui, registry);
  } finally {
    stop();
  }
}
