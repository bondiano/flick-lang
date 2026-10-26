// `Ui.Document`: the nodes of a root's elements, which the tasks of its
// handlers and instances reach. A handle whose node is not mounted, or is
// not a DOM element, fails with the Flick error `Ui.Detached`.
import { Effect } from "effect";

import { service } from "../../effects.ts";
import { tag } from "../../tags.ts";
import type { ElementHandle } from "./nodes.ts";

const DETACHED = /*#__PURE__*/ tag("Ui.Detached.Detached");

interface Node {
  readonly isConnected: boolean;
  focus(): void;
  blur(): void;
  scrollIntoView(): void;
  getBoundingClientRect(): { readonly width: number; readonly height: number; readonly top: number; readonly left: number };
}

const isNode = (node: unknown): node is Node =>
  typeof node === "object" && node !== null && Reflect.get(node, "isConnected") === true && typeof Reflect.get(node, "focus") === "function";

/** What `f` does with the mounted node of `handle`. */
const onNode =
  <A>(f: (node: Node) => A) =>
  (handle: ElementHandle): Effect.Effect<A, unknown> =>
    Effect.suspend(() => (isNode(handle.node) ? Effect.succeed(f(handle.node)) : Effect.fail({ _tag: DETACHED })));

const impl = {
  focus: onNode((node) => node.focus()),
  blur: onNode((node) => node.blur()),
  scroll_into_view: onNode((node) => node.scrollIntoView()),
  measure: onNode((node) => {
    const { width, height, top, left } = node.getBoundingClientRect();
    return { width, height, top, left };
  }),
};

/** The service `Ui.Document`. */
export const DocumentService = /*#__PURE__*/ service<typeof impl>("Ui.Document");

/** The implementation a root provides. */
export const document = impl;
