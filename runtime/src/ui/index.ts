// The DOM backend mounts synchronously once so event targets are immediately usable.
import { Effect } from "effect";
import type * as AtomRegistry from "effect/reactivity/AtomRegistry";
import * as Hydration from "effect/reactivity/Hydration";
import { flushSync } from "react-dom";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import type { Failure } from "../cause.ts";
import { RegistryService, nativeRegistry } from "../effect/reactivity/Atom.ts";
import { type Renderer, type Ui, uiMountWith } from "./react/index.ts";

export * from "./react/index.ts";

export const uiContainer = (id: string): Element | null => globalThis.document?.getElementById(id) ?? null;

/** A React DOM root in `container`, hydrating the markup a server rendered there. */
const domRenderer =
  (container: Element): Renderer =>
  () => {
      let root: Root | undefined;
      return {
        render: (node) => {
          if (root !== undefined) root.render(node);
          else {
            // React resets uncontrolled textareas during hydration; preserve native pre-boot drafts.
            const drafts = container.hasAttribute("data-flick-ssr")
              ? Array.from(container.querySelectorAll("textarea"), (field) => ({
                  field, value: field.value, start: field.selectionStart, end: field.selectionEnd, direction: field.selectionDirection,
                }))
              : undefined;
            flushSync(() => {
              if (drafts !== undefined) root = hydrateRoot(container, node);
              else {
                root = createRoot(container);
                root.render(node);
              }
            });
            if (drafts !== undefined) {
              for (const draft of drafts) {
                draft.field.value = draft.value;
                draft.field.setSelectionRange(draft.start, draft.end, draft.direction);
              }
            }
          }
        },
        unmount: () => root?.unmount(),
      };
  };

/**
 * The atoms `Ui.Server.state` wrote into the document, into `registry`.
 * The script is removed, so a later root does not write them over values
 * changed since.
 */
const hydrateState = (registry: AtomRegistry.AtomRegistry): void => {
  const script = globalThis.document?.getElementById("flick-state");
  if (script === null || script === undefined) return;
  script.remove();
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- `Ui.Server.state` wrote these entries
  Hydration.hydrate(registry, JSON.parse(script.textContent ?? "[]") as Array<Hydration.DehydratedAtom>);
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
  Effect.andThen(
    Effect.flatMap(Effect.service(RegistryService), (record) =>
      Effect.sync(() => {
        const registry = nativeRegistry(record);
        if (registry !== undefined) hydrateState(registry);
      }),
    ),
    uiMountWith(domRenderer(container), supervisor, ui),
  );
