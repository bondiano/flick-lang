// The DOM backend mounts synchronously once so event targets are immediately usable.
import type { Effect } from "effect";
import { flushSync } from "react-dom";
import { createRoot } from "react-dom/client";

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
      const root = createRoot(container);
      let first = true;
      return {
        render: (node) => {
          if (first) {
            first = false;
            flushSync(() => root.render(node));
          } else root.render(node);
        },
        unmount: () => root.unmount(),
      };
    },
    supervisor,
    model,
    view,
  );
