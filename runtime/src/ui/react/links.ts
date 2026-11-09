// The elements of `Ui` that make an Effect or an Effect's `Option`: a link
// that writes the route state, and a checked URL.
import { Effect, Option } from "effect";
import type * as Atom from "../../effect/reactivity/Atom.ts";

import { WriteService } from "../../effect/reactivity/Atom.ts";
import { routePrint, type UrlCodec } from "../../route.ts";
import { eventLatest } from "./handlers.ts";
import { type Ui, isSafeUrl, plainClick, routeLink } from "./nodes.ts";

interface RouteLinkProps {
  readonly route: Atom.Writable<unknown>;
  readonly url: UrlCodec;
  readonly to: Readonly<Record<string, unknown>>;
}

/**
 * A link inside the app: a plain click writes `to` to the route state, which
 * pushes its URL without a reload; another click, and markup without a
 * client, follow the real `href`.
 */
export const uiRouteLink = (props: RouteLinkProps, children: ReadonlyArray<Ui>): Ui => {
  const navigate = Effect.flatMap(Effect.service(WriteService), (write) => write.set("Ui.route_link")(props.route)(props.to));
  // a handler's task runs with the root's `Atom.Write`, as one of Flick's does
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the root provides the service
  const run = navigate as Effect.Effect<void>;
  const handler = eventLatest((snapshot) => (plainClick(snapshot) ? run : Effect.void));
  return routeLink(routePrint(props.url, props.to), handler, children);
};

/** `Ui.safe_url`: a relative URL, or one with a safe scheme. */
export const uiSafeUrl = (url: string): Option.Option<string> => (isSafeUrl(url) ? Option.some(url) : Option.none());
