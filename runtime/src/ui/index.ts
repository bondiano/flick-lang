// The DOM backend mounts synchronously once so event targets are immediately usable.
import { Effect } from "effect";
import { flushSync } from "react-dom";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import type { Cell } from "../builtins.ts";
import type { Failure } from "../cause.ts";
import { nativeRegistry } from "../effect/reactivity/Atom.ts";
import { service } from "../effects.ts";
import { type Renderer, type Ui, uiRunWith } from "./react/index.ts";

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

export const uiRun = <M, R>(
  container: Element,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  model: Cell<M>,
  view: (model: M) => Ui,
): Effect.Effect<never, unknown, R> => uiRunWith(domRenderer(container), supervisor, model, view);

/** The service `Reactivity.Atom.Registry`, whose registry views read atoms from. */
const RegistryService = /*#__PURE__*/ service<object>("Reactivity.Atom.Registry");

/**
 * `Ui.Dom.mount`: renders `ui` into `container` with the registry of the
 * provided `Reactivity.Atom.Registry`; a view in it renders again when an
 * atom it read changes. Events and shutdown work as in `uiRun`.
 */
export const uiMount = <R>(
  container: Element,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  ui: Ui,
): Effect.Effect<never, unknown, unknown> =>
  Effect.flatMap(Effect.service(RegistryService), (record) => {
    const registry = nativeRegistry(record);
    if (registry === undefined) return Effect.die(new Error("`Reactivity.Atom.Registry` was not built by its `layer`"));
    return uiRunWith(domRenderer(container), supervisor, { value: ui }, (node: Ui) => node, registry);
  });
