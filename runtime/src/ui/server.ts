// Server markup: a `Ui` rendered with the registry of one request. A
// separate entry point, so client bundles do not keep `react-dom/server`.
import { Effect, Stream } from "effect";
import * as AsyncResult from "effect/reactivity/AsyncResult";
import * as Atom from "effect/reactivity/Atom";
import type * as AtomRegistry from "effect/reactivity/AtomRegistry";
import { createElement } from "react";
import { renderToReadableStream, renderToString } from "react-dom/server";

import { handedOver } from "../atoms.ts";
import { RegistryService, nativeRegistry } from "../effect/reactivity/Atom.ts";
import type { Ui } from "./react/nodes.ts";
import { RegistryContext, StreamContext, uiRenderStatic } from "./react/render.ts";

/**
 * `Ui.Server.render`: the markup of `ui`, its views reading the atoms of
 * the provided registry. Each `provide Atom.layer` is a registry
 * of its own, so requests rendered in parallel do not see each other's atoms.
 */
export const uiServerRender = (ui: Ui): Effect.Effect<string, never, unknown> =>
  withRegistry((registry) => renderToString(createElement(RegistryContext.Provider, { value: registry }, uiRenderStatic(ui))));

/** The entry of the serializable `atom` whose value is `value`, if a server hands it over. */
const entryOf = (key: string, atom: Atom.Atom<unknown>, value: unknown): Array<{ key: string; value: unknown }> => {
  if (!Atom.isSerializable(atom)) return [];
  // a task not yet run, or a task atom not loaded, has no value to hand over; the client runs it
  if ((AsyncResult.isAsyncResult(value) && AsyncResult.isInitial(value)) || !handedOver(atom, value)) return [];
  try {
    return [{ key, value: atom[Atom.SerializableTypeId].encode(value) }];
  } catch (cause) {
    throw new Error(`the serializable atom \`${key}\` does not encode its value`, { cause });
  }
};

/** The serializable atoms of the registry as entries of `Hydration.hydrate`. */
const entries = (registry: AtomRegistry.AtomRegistry): Array<{ key: string; value: unknown }> => {
  const all: Array<{ key: string; value: unknown }> = [];
  registry.getNodes().forEach((node, key) => all.push(...entryOf(String(key), node.atom, node.value())));
  return all;
};

/** A JSON script of `entries` for the root in `container`, with `attribute` naming it. */
const script = (attribute: string, container: string, list: ReadonlyArray<{ key: string; value: unknown }>): string => {
  // `<` escaped, so no value closes the script
  const json = JSON.stringify(list).replaceAll("<", "\\u003c");
  const name = container.replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;");
  return `<script type="application/json" ${attribute}="${attribute === "id" ? "flick-state-" : ""}${name}">${json}</script>`;
};

/**
 * `Ui.Server.state`: the serializable atoms of the registry, as entries of
 * `Hydration.hydrate` in a JSON script. Unlike `Hydration.dehydrate`, an
 * atom whose value does not encode is a defect naming its key, not skipped.
 * The script's id is `flick-state-` and the container's, so each root
 * `Ui.Dom.mount` mounts reads its own.
 */
export const uiServerState = (container: string): Effect.Effect<string, never, unknown> =>
  withRegistry((registry) => script("id", container, entries(registry)));

/**
 * `Ui.Server.stream`: the container `<div id="container">` with the markup
 * of `ui`, in chunks as React renders them. A view of `Ui.suspended` that
 * reads a loading task is sent as its fallback, and its markup follows once
 * the task ends. The serializable atoms come in JSON scripts in the stream:
 * those of the shell before it, a serializable task's result as soon as it
 * loads, before the markup that shows it, so the client hydrates that
 * markup from the same value. The stream renders the container itself:
 * React flushes no shell whose root is not an element.
 */
export const uiServerStream = (container: string, ui: Ui): Stream.Stream<string, never, unknown> =>
  Stream.unwrap(
    withRegistry((registry) =>
      Stream.orDie(Stream.fromReadableStream({ evaluate: () => markupOf(registry, container, ui), onError: (cause) => cause })),
    ),
  );

const markupOf = (registry: AtomRegistry.AtomRegistry, container: string, ui: Ui): ReadableStream<string> => {
  const watched = new Map<Atom.Atom<unknown>, () => void>();
  const waiting = new Map<Atom.Atom<unknown>, Array<() => void>>();
  const release = (): void => {
    for (const off of watched.values()) off();
    watched.clear();
  };
  return new ReadableStream<string>({
    start: async (out) => {
      let failed = false;
      const fail = (error: unknown): void => {
        if (failed) return;
        failed = true;
        release();
        out.error(error);
      };
      const send = (list: ReadonlyArray<{ key: string; value: unknown }>): void => {
        if (list.length > 0 && !failed) out.enqueue(script("data-flick-state", container, list));
      };
      // the subscription keeps a task running until the stream ends; its change is sent before React's chunk
      const watch = (atom: Atom.Atom<unknown>): void => {
        if (watched.has(atom)) return;
        const key = Atom.isSerializable(atom) ? atom[Atom.SerializableTypeId].key : undefined;
        const off = registry.subscribe(atom, (value) => {
          try {
            if (key !== undefined) send(entryOf(key, atom, value));
          } catch (error) {
            fail(error);
          }
          const resume = waiting.get(atom) ?? [];
          waiting.delete(atom);
          for (const go of resume) go();
        });
        watched.set(atom, off);
      };
      const streaming = {
        wait: (atoms: ReadonlyArray<Atom.Atom<unknown>>): Promise<void> =>
          new Promise<void>((resolve) => {
            for (const atom of atoms) {
              watch(atom);
              waiting.set(atom, [...(waiting.get(atom) ?? []), resolve]);
            }
          }),
      };
      const tree = createElement(
        RegistryContext.Provider,
        { value: registry },
        createElement(
          "div",
          { id: container, "data-flick-ssr": "" },
          createElement(StreamContext.Provider, { value: streaming }, uiRenderStatic(ui)),
        ),
      );
      try {
        // an error in a boundary after the shell fails the stream, as one in the shell does
        const react = await renderToReadableStream(tree, { onError: fail });
        send(entries(registry));
        const reader = react.pipeThrough(new TextDecoderStream()).getReader();
        for (;;) {
          // oxlint-disable-next-line eslint/no-await-in-loop -- chunks are read one after another
          const { done, value } = await reader.read();
          if (done || failed) break;
          out.enqueue(value);
        }
        if (!failed) out.close();
      } catch (error) {
        fail(error);
      } finally {
        release();
      }
    },
    cancel: release,
  });
};

const withRegistry = <A>(f: (registry: AtomRegistry.AtomRegistry) => A): Effect.Effect<A, never, unknown> =>
  Effect.flatMap(Effect.service(RegistryService), (record) => {
    const registry = nativeRegistry(record);
    if (registry === undefined) return Effect.die(new Error("`Reactivity.Atom.Registry` was not built by its `layer`"));
    return Effect.sync(() => f(registry));
  });
