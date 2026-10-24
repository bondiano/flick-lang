import type { Ui } from "@flick-lang/runtime/ui/react";

/** Host attributes only: Flick retains the node's state and event handlers. */
export const attributes = (pairs: ReadonlyArray<readonly [string, string]>, node: Ui): Ui => {
  if (node.kind !== "element") throw new TypeError("Host attributes require a direct Ui element");
  return Object.freeze({ ...node, attrs: Object.freeze({ ...node.attrs, ...Object.fromEntries(pairs) }) });
};

/** Page-provided configuration; decoding and all mode/locale behavior belong to Flick. */
export const configuration = (): { mode: string; locale: string } => {
  const root = document.getElementById("flick-playground");
  if (root === null) throw new Error("The page has no flick-playground container");
  return { mode: root.dataset.mode ?? "", locale: root.dataset.locale ?? "" };
};

/** Read native text entered before hydration; Flick adopts it into its model. */
export const sourceValue = (): string => {
  const editor = document.querySelector<HTMLTextAreaElement>("#flick-playground textarea");
  if (editor === null) throw new Error("The playground source editor is missing");
  return editor.value;
};

/** Queue a host event so a mounted Flick root can render its changed Ref. */
export const notify = (id: string): void => {
  queueMicrotask(() => document.getElementById(id)?.click());
};
