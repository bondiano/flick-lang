// `Ui.Server.to_html` on Flick's own runtime: a `Ui` as HTML without React.
import { html } from "./markup.ts";
import type { Ui } from "./react/nodes.ts";
import { type Atom, Registry } from "../atoms/registry.ts";
import { currentCtx } from "../fiber.ts";

/**
 * The HTML of `ui`. Text and attribute values are escaped; views read atoms
 * as a server does, from a registry of this call alone, so the same `ui`
 * always gives the same HTML.
 */
export const uiToHtml = (ui: Ui): string => {
  const registry = new Registry(currentCtx());
  try {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a view reads atoms
    return html(ui, (atom) => registry.serverValue(atom as Atom));
  } finally {
    registry.dispose();
  }
};
