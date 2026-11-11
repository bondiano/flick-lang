// Server markup on Flick's own runtime: a `Ui` rendered with the registry of
// one request.
import { createElement } from "react";
import { renderToString } from "react-dom/server";

import { script } from "../../ui/markup.ts";
import type { Ui } from "../../ui/react/nodes.ts";
import { entries } from "../atoms/serial.ts";
import { registryHere } from "../atoms/services.ts";
import { absent } from "../builtins.ts";
import { RegistryContext, uiRenderStatic } from "./render.ts";

/** `Ui.Server.render`: the markup of `ui`, its views reading the atoms of the provided registry. */
export const uiServerRender = (ui: Ui): string =>
  renderToString(createElement(RegistryContext.Provider, { value: registryHere() }, uiRenderStatic(ui)));

/**
 * `Ui.Server.state`: the serializable atoms of the registry, in a JSON
 * script whose id is `flick-state-` and the container's, so each root
 * `Ui.Dom.mount` mounts reads its own. An atom whose value does not encode is
 * a defect naming its key.
 */
export const uiServerState = (container: string): string => script("id", container, entries(registryHere()));

// a stream of markup is a `Stream`, which this runtime does not have yet
export const uiServerStream = /*#__PURE__*/ absent("@flick-lang/effect-ts/ui/server", "uiServerStream");
