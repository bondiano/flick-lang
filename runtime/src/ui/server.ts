// Server markup: a `Ui` rendered with the registry of one request. A
// separate entry point, so client bundles do not keep `react-dom/server`.
import { Effect } from "effect";
import { createElement } from "react";
import { renderToString } from "react-dom/server";

import { RegistryService, nativeRegistry } from "../effect/reactivity/Atom.ts";
import type { Ui } from "./react/nodes.ts";
import { RegistryContext, uiRenderStatic } from "./react/render.ts";

/**
 * `Ui.Server.render`: the markup of `ui`, its views reading the atoms of
 * the provided registry. Each `provide Reactivity.Atom.layer` is a registry
 * of its own, so requests rendered in parallel do not see each other's atoms.
 */
export const uiServerRender = (ui: Ui): Effect.Effect<string, never, unknown> =>
  Effect.flatMap(Effect.service(RegistryService), (record) => {
    const registry = nativeRegistry(record);
    if (registry === undefined) return Effect.die(new Error("`Reactivity.Atom.Registry` was not built by its `layer`"));
    return Effect.sync(() => renderToString(createElement(RegistryContext.Provider, { value: registry }, uiRenderStatic(ui))));
  });
