// The DOM backend mounts synchronously once so event targets are immediately usable.
import type { Effect } from "effect";
import { flushSync } from "react-dom";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import type { Failure } from "../cause.ts";
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
 * `Ui.Dom.mount`: renders `ui` into `container` with the registry of the
 * provided `Reactivity.Atom.Registry`, hydrating markup a server rendered.
 */
export const uiMount = <R>(
  container: Element,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  ui: Ui,
): Effect.Effect<never, unknown, unknown> => uiMountWith(domRenderer(container), supervisor, ui);
