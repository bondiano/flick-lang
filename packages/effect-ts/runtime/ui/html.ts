// Templates: a `Ui` as HTML without React, for mail and other markup no
// client hydrates. A separate entry point that imports neither React nor
// `react-dom/server`.
import * as Atom from "../effect/reactivity/Atom.ts";
import * as AtomRegistry from "../effect/reactivity/AtomRegistry.ts";

import { html } from "./markup.ts";
import type { Ui } from "./react/nodes.ts";

/**
 * `Ui.Server.to_html`: the HTML of `ui`. Text and attribute values are
 * escaped; views read atoms as a server does, from a registry of this call
 * alone, so the same `ui` always gives the same HTML.
 */
export const uiToHtml = (ui: Ui): string => {
  const registry = AtomRegistry.make();
  try {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a view reads atoms of this registry
    return html(ui, (atom) => Atom.getServerValue(atom as Atom.Atom<unknown>, registry));
  } finally {
    registry.dispose();
  }
};
