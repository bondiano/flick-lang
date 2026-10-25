// Server markup: a `Ui` rendered with the registry of one request. A
// separate entry point, so client bundles do not keep `react-dom/server`.
import { Effect } from "effect";
import * as AsyncResult from "effect/reactivity/AsyncResult";
import * as Atom from "effect/reactivity/Atom";
import type * as AtomRegistry from "effect/reactivity/AtomRegistry";
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
  withRegistry((registry) => renderToString(createElement(RegistryContext.Provider, { value: registry }, uiRenderStatic(ui))));

/**
 * `Ui.Server.state`: the serializable atoms of the registry, as entries of
 * `Hydration.hydrate` in a JSON script. Unlike `Hydration.dehydrate`, an
 * atom whose value does not encode is a defect naming its key, not skipped.
 * `Ui.Dom.mount` reads the script by its id `flick-state`.
 */
export const uiServerState = (): Effect.Effect<string, never, unknown> =>
  withRegistry((registry) => {
    const entries: Array<{ key: string; value: unknown }> = [];
    registry.getNodes().forEach((node, key) => {
      if (!Atom.isSerializable(node.atom)) return;
      const value: unknown = node.value();
      // a task not yet run has no value to hand over; the client runs it
      if (AsyncResult.isAsyncResult(value) && AsyncResult.isInitial(value)) return;
      try {
        entries.push({ key: String(key), value: node.atom[Atom.SerializableTypeId].encode(value) });
      } catch (cause) {
        throw new Error(`the serializable atom \`${String(key)}\` does not encode its value`, { cause });
      }
    });
    // `<` escaped, so no value closes the script
    const json = JSON.stringify(entries).replaceAll("<", "\\u003c");
    return `<script type="application/json" id="flick-state">${json}</script>`;
  });

const withRegistry = <A>(f: (registry: AtomRegistry.AtomRegistry) => A): Effect.Effect<A, never, unknown> =>
  Effect.flatMap(Effect.service(RegistryService), (record) => {
    const registry = nativeRegistry(record);
    if (registry === undefined) return Effect.die(new Error("`Reactivity.Atom.Registry` was not built by its `layer`"));
    return Effect.sync(() => f(registry));
  });
