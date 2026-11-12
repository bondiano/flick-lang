import type { Ui } from "@flick-lang/runtime/ui/react/nodes";

/** Host attributes `Ui.Host` does not type (`title`, `popoverTarget`, …): Flick retains the node's state and event handlers. */
export const attributes = (pairs: ReadonlyArray<readonly [string, string]>, node: Ui): Ui => {
  if (node.kind !== "element") throw new TypeError("Host attributes require a direct Ui element");
  return Object.freeze({ ...node, attrs: Object.freeze({ ...node.attrs, ...Object.fromEntries(pairs) }) });
};
