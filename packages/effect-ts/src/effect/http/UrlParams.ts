// Effect's `UrlParams` for the bindings of the standard library.
import { Effect } from "effect";
import * as UrlParams from "effect/http/UrlParams";

import { masked, redaction, showPairs } from "./convert.ts";

export * from "effect/http/UrlParams";

/** `Http.UrlParams.to_record`: every key with all its values. */
export const flickToRecord = (params: UrlParams.UrlParams): ReadonlyArray<readonly [string, ReadonlyArray<string>]> => {
  const out = new Map<string, Array<string>>();
  for (const [k, v] of params.params) {
    const values = out.get(k);
    if (values === undefined) out.set(k, [masked(v)]);
    else values.push(masked(v));
  }
  return [...out];
};

/** `Http.UrlParams.redact names params`. */
export const flickRedact = (names: ReadonlyArray<string>, params: UrlParams.UrlParams) =>
  UrlParams.redact(params, names).map(([k, v]) => [k, redaction(v)] as const);

/** `Http.UrlParams.redact_url names url`. */
export const flickRedactUrl = (names: ReadonlyArray<string>, url: string): string => UrlParams.redactUrl(url, names);

/** `Http.UrlParams.with_redacted_names names task`. */
export const flickWithRedactedNames = <A, E, R>(names: ReadonlyArray<string>, effect: Effect.Effect<A, E, R>) =>
  Effect.provideService(effect, UrlParams.CurrentRedactedNames, names);

/** `Http.UrlParams.default_redacted_names`. */
export const flickDefaultRedactedNames: ReadonlyArray<string> = /*#__PURE__*/ UrlParams.CurrentRedactedNames.defaultValue().filter(
  (name): name is string => typeof name === "string",
);

/** The pairs of the parameters. */
export const flickToList = (params: UrlParams.UrlParams): ReadonlyArray<readonly [string, string]> =>
  params.params.map(([k, v]) => [k, masked(v)] as const);

/** A key's value as a string, or as frozen strings for a key given more than once. */
export const record = (params: UrlParams.UrlParams): Readonly<Record<string, string | ReadonlyArray<string>>> =>
  Object.fromEntries(flickToRecord(params).map(([k, vs]) => [k, vs.length === 1 ? (vs[0] ?? "") : Object.freeze(vs)]));

/** The parameters as inert data for a codec. */
export const flickToUnknown = (params: UrlParams.UrlParams): unknown => Object.freeze(record(params));

/** `show` of parameters, the default secret names masked. */
export const flickShow = (params: UrlParams.UrlParams): string =>
  showPairs("UrlParams", UrlParams.redact(params, UrlParams.CurrentRedactedNames.defaultValue()));
