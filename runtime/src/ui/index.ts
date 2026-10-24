// The DOM backend mounts synchronously once so event targets are immediately usable.
import type { Effect } from "effect";
import { flushSync } from "react-dom";
import { createRoot, hydrateRoot, type Root } from "react-dom/client";

import type { Cell } from "../builtins.ts";
import type { Failure } from "../cause.ts";
import { type Ui, uiRunWith } from "./react/index.ts";

export * from "./react/index.ts";

export const uiContainer = (id: string): Element | null => globalThis.document?.getElementById(id) ?? null;

export const uiRun = <M, R>(
  container: Element,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
  model: Cell<M>,
  view: (model: M) => Ui,
): Effect.Effect<never, unknown, R> =>
  uiRunWith(
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
    },
    supervisor,
    model,
    view,
  );
