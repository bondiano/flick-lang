// Server markup on Flick's own runtime: a `Ui` rendered with the registry of
// one request.
import { createElement } from "react";
import { renderToReadableStream, renderToString } from "react-dom/server";

import { script } from "../../ui/markup.ts";
import type { Ui } from "../../ui/react/nodes.ts";
import { entries } from "../atoms/serial.ts";
import { registryHere } from "../atoms/services.ts";
import type { Atom, Registry } from "../atoms/registry.ts";
import { RegistryContext, StreamContext, type Streaming, uiRenderStatic } from "./render.ts";
import { streamHooks } from "./stream_hook.ts";

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

/** One entry of the serializable `atom` whose value is `value`, if a server hands it over. */
const entryOf = (atom: Atom, value: unknown): Array<{ key: string; value: unknown }> =>
  atom.serial === undefined || !atom.serial.handedOver(value) ? [] : [{ key: atom.serial.key, value: atom.serial.encode(value) }];

/**
 * `Ui.Server.stream`: the container `<div id="container">` with the markup
 * of `ui`, in chunks as React renders them. A view of `Ui.suspended` that
 * reads a loading task is sent as its fallback, and its markup follows once
 * the task ends. The serializable atoms come in JSON scripts in the stream:
 * those of the shell before it, a serializable task's result as soon as it
 * loads, before the markup that shows it, so the client hydrates that markup
 * from the same value. The stream is the adapter's `Stream`; it reads the
 * registry when it starts.
 */
export const uiServerStream = (container: string, ui: Ui): unknown => {
  if (streamHooks.text === undefined) throw new Error("`Ui.Server.stream` makes a `Stream`, which needs the optional `@flick-lang/effect-ts` adapter");
  return streamHooks.text(() => markupOf(registryHere(), container, ui));
};

const markupOf = (registry: Registry, container: string, ui: Ui): ReadableStream<string> => {
  const watched = new Map<Atom, () => void>();
  const waiting = new Map<Atom, Array<() => void>>();
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
      const watch = (atom: Atom): void => {
        if (watched.has(atom)) return;
        const off = registry.subscribe(atom, (value) => {
          try {
            send(entryOf(atom, value));
          } catch (error) {
            fail(error);
          }
          const resume = waiting.get(atom) ?? [];
          waiting.delete(atom);
          for (const go of resume) go();
        });
        watched.set(atom, off);
      };
      const streaming: Streaming = {
        wait: (read) =>
          new Promise<void>((resolve) => {
            for (const [atom, value] of read) {
              watch(atom);
              // a change between the read and the subscription would not call it
              if (!Object.is(registry.serverValue(atom), value)) return resolve();
            }
            for (const atom of read.keys()) waiting.set(atom, [...(waiting.get(atom) ?? []), resolve]);
          }),
      };
      const tree = createElement(
        RegistryContext.Provider,
        { value: registry },
        createElement("div", { id: container, "data-flick-ssr": "" }, createElement(StreamContext.Provider, { value: streaming }, uiRenderStatic(ui))),
      );
      try {
        // an error in a boundary after the shell fails the stream, as one in the shell does
        const react = await renderToReadableStream(tree, { onError: fail });
        send(entries(registry));
        // decoded here, not by a `TextDecoderStream`: closing one makes an error per response
        const decoder = new TextDecoder();
        // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- React's stream is of bytes, typed `any`
        const reader = (react as ReadableStream<Uint8Array>).getReader();
        for (;;) {
          // oxlint-disable-next-line eslint/no-await-in-loop -- chunks are read one after another
          const { done, value } = await reader.read();
          if (failed) break;
          const text = done ? decoder.decode() : decoder.decode(value, { stream: true });
          if (text !== "") out.enqueue(text);
          if (done) break;
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
