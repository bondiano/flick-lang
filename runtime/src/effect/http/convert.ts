// Conversions shared by the bindings of Effect's HTTP modules: Flick
// enumerations to Effect's string literals and back, option records to
// Effect's optional fields, and the masked text of redacted values.
import { Option, Redacted } from "effect";
import type { HttpMethod } from "effect/http/HttpMethod";

import type { Data } from "../../data.ts";
import { tag } from "../../tags.ts";

/** A Flick type of nullary constructors and the JS strings Effect uses. */
export const enumeration = <T extends string>(type: string, names: ReadonlyArray<readonly [string, T]>) => {
  const toJs = new Map(names.map(([name, js]) => [tag(`${type}.${name}`), js]));
  const toFlick = new Map(names.map(([name, js]) => [js, Object.freeze({ _tag: tag(`${type}.${name}`) })]));
  return {
    js: (value: Data): T => {
      const js = toJs.get(value._tag);
      if (js === undefined) throw new TypeError(`not a ${type}: ${value._tag}`);
      return js;
    },
    flick: (js: T): Data => {
      const value = toFlick.get(js);
      if (value === undefined) throw new TypeError(`not a ${type}: ${js}`);
      return value;
    },
  };
};

const METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE", "QUERY"] as const;

/** `Http.Method.Method`. */
export const method = enumeration<HttpMethod>(
  "Http.Method.Method",
  METHODS.map((m) => [m, m] as const),
);

type Defined<T> = { readonly [K in keyof T]: T[K] extends Option.Option<infer A> ? A | undefined : T[K] };

/** A Flick options record with `Option` fields as Effect's optional fields. */
export const defined = <T extends object>(options: T): Defined<T> => {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(options)) {
    out[key] = Option.isOption(value) ? Option.getOrUndefined(value) : value;
  }
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- each field was copied, the options unwrapped
  return out as Defined<T>;
};

/** `Some` of a defined value. */
export const option = <A>(value: A | undefined): Option.Option<A> => Option.fromUndefinedOr(value);

/** A value Effect may have redacted, as text that never shows the secret. */
export const masked = (value: unknown): string => (Redacted.isRedacted(value) ? "<redacted>" : String(value));

/** Pairs as `[key, value]` text, values masked: `[("a", "1"), ("b", "<redacted>")]`. */
export const showPairs = (name: string, pairs: Iterable<readonly [string, unknown]>): string =>
  `${name} [${Array.from(pairs, ([k, v]) => `(${JSON.stringify(k)}, ${JSON.stringify(masked(v))})`).join(", ")}]`;

const PLAIN = tag("Http.Headers.Redaction.Plain");
const SECRET = tag("Http.Headers.Redaction.Secret");

/** `Http.Headers.Redaction` of a value Effect may have redacted. */
export const redaction = (value: unknown): Data =>
  Redacted.isRedacted(value) ? { _tag: SECRET, _0: value } : { _tag: PLAIN, _0: String(value) };

/** The bytes of a Flick `List Number`. */
export const toBytes = (bytes: ReadonlyArray<number>): Uint8Array => Uint8Array.from(bytes);

/** A Flick `List Number` of the bytes. */
export const fromBytes = (bytes: Uint8Array | ArrayBuffer): ReadonlyArray<number> =>
  Array.from(bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes));
