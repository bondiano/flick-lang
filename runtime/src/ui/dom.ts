// The DOM backend: a React DOM root in a container, mounted synchronously once
// so event targets are immediately usable.
import { flushSync } from "react-dom";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import type { Renderer } from "./react/tree.ts";

export const uiContainer = (id: string): Element | null => globalThis.document?.getElementById(id) ?? null;

/** A React DOM root in `container`, hydrating the markup a server rendered there. */
export const domRenderer =
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
