// Effect's `Headers` for the bindings of the standard library.
import { Effect } from "effect";
import * as Headers from "effect/http/Headers";

import { redaction, showPairs } from "./convert.ts";

export * from "effect/http/Headers";

/** `Http.Headers.redact names headers`. */
export const flickRedact = (names: ReadonlyArray<string>, headers: Headers.Headers) =>
  Object.entries(Headers.redact(headers, names)).map(([k, v]) => [k, redaction(v)] as const);

/** `Http.Headers.with_redacted_names names task`. */
export const flickWithRedactedNames = <A, E, R>(names: ReadonlyArray<string>, effect: Effect.Effect<A, E, R>) =>
  Effect.provideService(effect, Headers.CurrentRedactedNames, names);

/** The pairs of the headers. */
export const flickToList = (headers: Headers.Headers): ReadonlyArray<readonly [string, string]> =>
  Object.entries(headers);

/** The headers as inert data for a codec. */
export const flickToUnknown = (headers: Headers.Headers): unknown => Object.freeze({ ...headers });

/** `show` of headers, the default secret names masked. */
export const flickShow = (headers: Headers.Headers): string =>
  showPairs("Headers", Object.entries(Headers.redact(headers, Headers.CurrentRedactedNames.defaultValue())));
