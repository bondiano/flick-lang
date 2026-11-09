// `@flick-lang/runtime/ui` on Flick's own runtime: a root in a DOM container.
import type { Ui } from "../ui/react/nodes.ts";
import { domRenderer } from "../ui/dom.ts";
import { registryHere } from "../atoms/services.ts";
import type { Run } from "../fiber.ts";
import type { Failure } from "../outcome.ts";
import { uiRunWith } from "./react.ts";

export * from "./react.ts";
export { uiContainer } from "../ui/dom.ts";

/**
 * `Ui.Dom.mount`: renders `ui` into `container` with the registry of the
 * provided `Reactivity.Atom.Registry`, hydrating markup a server rendered.
 */
export const uiMount = (container: Element, supervisor: (failure: Failure) => unknown, ui: Ui): Run =>
  uiRunWith(domRenderer(container), supervisor, ui, registryHere());
