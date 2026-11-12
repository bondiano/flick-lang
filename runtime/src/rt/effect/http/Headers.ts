// `Http.Headers` on Flick's own runtime, as Effect's `Headers` behaves:
// names are lowercase, a later value of a name replaces the earlier one, and
// `show` masks the values of the secret names.
import { type Option, none, some } from "../../../core/data.ts";
import { tag } from "../../../tags.ts";
import { Under, currentCtx, extend, lookup } from "../../fiber.ts";
import { type Redacted, redactedMake } from "../../redacted.ts";

/** Header values by lowercase name, in the order they were first set. */
export type Headers = Readonly<Record<string, string>>;

type Input = ReadonlyArray<readonly [string, string]>;

const make = (pairs: Iterable<readonly [string, string]>): Headers => {
  const out: Record<string, string> = {};
  // without a prototype, a client's header named `__proto__` is a header like any other
  Object.setPrototypeOf(out, null);
  for (const [name, value] of pairs) out[name.toLowerCase()] = value;
  return Object.freeze(out);
};

const pairs = (headers: Headers): Array<[string, string]> => Object.entries(headers);

export const empty: Headers = /*#__PURE__*/ make([]);
export const fromInput = (input: Input): Headers => make(input);
export const has = (name: string) => (headers: Headers): boolean => Object.hasOwn(headers, name.toLowerCase());
export const get = (name: string) => (headers: Headers): Option<string> => {
  const value = headers[name.toLowerCase()];
  return value === undefined ? none : some(value);
};
export const set = (name: string, value: string) => (headers: Headers): Headers => make([...pairs(headers), [name, value]]);
export const setAll = (input: Input) => (headers: Headers): Headers => make([...pairs(headers), ...input]);
/** `merge other headers`: the values of `other` win. */
export const merge = (other: Headers) => (headers: Headers): Headers => make([...pairs(headers), ...pairs(other)]);
export const remove = (name: string) => (headers: Headers): Headers => {
  const gone = name.toLowerCase();
  return make(pairs(headers).filter(([n]) => n !== gone));
};
export const removeMany = (names: ReadonlyArray<string>) => (headers: Headers): Headers => {
  const gone = new Set(names.map((n) => n.toLowerCase()));
  return make(pairs(headers).filter(([n]) => !gone.has(n)));
};

/** The names whose values `show` and the logs mask, as Effect's `CurrentRedactedNames` has them. */
export const DEFAULT_REDACTED_NAMES: ReadonlyArray<string> = /*#__PURE__*/ Object.freeze([
  "authorization",
  "cookie",
  "set-cookie",
  "x-api-key",
]);

// the key of Effect's reference: the adapter's code reads the same names
const REDACTED_NAMES = /*#__PURE__*/ Object.freeze({ key: "effect/Headers/CurrentRedactedNames" });

/** The secret names of the running task. */
export const redactedNames = (): ReadonlyArray<string> => {
  const names: unknown = lookup(REDACTED_NAMES);
  return Array.isArray(names) ? names.map(String) : DEFAULT_REDACTED_NAMES;
};

export const flickWithRedactedNames = (names: ReadonlyArray<string>, task: () => unknown): Under =>
  new Under(extend(currentCtx(), REDACTED_NAMES, names), task);

export const isRedactedName = (name: string, names: ReadonlyArray<string>): boolean => {
  const lower = name.toLowerCase();
  return names.some((n) => n.toLowerCase() === lower);
};

/** `headers` as pairs, the values of `names` redacted. */
export const redacted = (names: ReadonlyArray<string>, headers: Headers): Array<[string, string | Redacted<string>]> =>
  pairs(headers).map(([n, v]) => [n, isRedactedName(n, names) ? redactedMake(v) : v]);

const PLAIN = /*#__PURE__*/ tag("Http.Headers.Redaction.Plain");
const SECRET = /*#__PURE__*/ tag("Http.Headers.Redaction.Secret");

export const flickRedact = (names: ReadonlyArray<string>, headers: Headers) =>
  redacted(names, headers).map(([n, v]) => [n, typeof v === "string" ? { _tag: PLAIN, _0: v } : { _tag: SECRET, _0: v }] as const);

export const flickToList = (headers: Headers): Input => pairs(headers);
export const flickToUnknown = (headers: Headers): unknown => Object.freeze({ ...headers });

export const Equivalence = (a: Headers, b: Headers): boolean => {
  const names = Object.keys(a);
  return names.length === Object.keys(b).length && names.every((n) => Object.hasOwn(b, n) && a[n] === b[n]);
};

/** Pairs as `Name [("a", "1"), ("b", "<redacted>")]`, as the adapter shows them. */
export const showPairs = (name: string, entries: Iterable<readonly [string, unknown]>): string =>
  `${name} [${Array.from(entries, ([k, v]) => `(${JSON.stringify(k)}, ${JSON.stringify(String(v))})`).join(", ")}]`;

export const flickShow = (headers: Headers): string => showPairs("Headers", redacted(DEFAULT_REDACTED_NAMES, headers));
