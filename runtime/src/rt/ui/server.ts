// Server markup on Flick's own runtime: a `Ui` rendered with the registry of
// one request.
import { createElement } from "react";
import { renderToString } from "react-dom/server";

import { script } from "../../ui/markup.ts";
import type { Ui } from "../../ui/react/nodes.ts";
import { registryHere } from "../atoms/services.ts";
import { absent } from "../builtins.ts";
import { RegistryContext, uiRenderStatic } from "./render.ts";

/** `Ui.Server.render`: the markup of `ui`, its views reading the atoms of the provided registry. */
export const uiServerRender = (ui: Ui): string =>
  renderToString(createElement(RegistryContext.Provider, { value: registryHere() }, uiRenderStatic(ui)));

/**
 * `Ui.Server.state`: the serializable atoms of the registry, in a JSON script
 * for the root in `container`: none, as no atom is serializable on this
 * runtime without a `Schema` to code it.
 */
export const uiServerState = (container: string): string => {
  registryHere();
  return script("id", container, []);
};

// a stream of markup is a `Stream`, which this runtime does not have yet
export const uiServerStream = /*#__PURE__*/ absent("@flick-lang/runtime/ui/server", "uiServerStream");
