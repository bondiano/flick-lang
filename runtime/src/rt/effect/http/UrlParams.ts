// `Http.UrlParams` as the list of its pairs of key and value, in order: a
// Flick list cannot change, so a pair list is already the immutable value
// Effect's `UrlParams` wraps. Redaction follows Effect's `UrlParams`: the
// names match ignoring case, and `redact_url` masks a URL's user info and
// the values of the named query parameters.
import { type Option, none, some } from "../../../core/data.ts";
import { tag } from "../../../tags.ts";
import { currentCtx, extend, lookup, Under } from "../../fiber.ts";
import { redactedMake } from "../../redacted.ts";

export type UrlParams = ReadonlyArray<readonly [string, string]>;

export const make = (input: UrlParams): UrlParams => input;
export const fromInput = make;
export const empty: UrlParams = /*#__PURE__*/ Object.freeze([]);

export const getAll = (key: string) => (self: UrlParams): ReadonlyArray<string> =>
  self.filter(([k]) => k === key).map(([, v]) => v);
const found = (pair: readonly [string, string] | undefined): Option<string> => (pair === undefined ? none : some(pair[1]));
export const getFirst = (key: string) => (self: UrlParams): Option<string> => found(self.find(([k]) => k === key));
export const getLast = (key: string) => (self: UrlParams): Option<string> => found(self.findLast(([k]) => k === key));

export const set = (key: string, value: string) => (self: UrlParams): UrlParams => [...self.filter(([k]) => k !== key), [key, value]];
export const transform = (f: (input: UrlParams) => UrlParams) => (self: UrlParams): UrlParams => f(self);
/** The pairs of `input`, then those of `self` whose key `input` has not. */
export const setAll = (input: UrlParams) => (self: UrlParams): UrlParams => {
  const keys = new Set(input.map(([k]) => k));
  return [...input, ...self.filter(([k]) => !keys.has(k))];
};
export const append = (key: string, value: string) => (self: UrlParams): UrlParams => [...self, [key, value]];
export const appendAll = (input: UrlParams) => (self: UrlParams): UrlParams => [...self, ...input];
export const remove = (key: string) => (self: UrlParams): UrlParams => self.filter(([k]) => k !== key);
export const toString = (self: UrlParams): string => new URLSearchParams(self.map(([k, v]) => [k, v])).toString();

export const flickToRecord = (self: UrlParams): ReadonlyArray<readonly [string, ReadonlyArray<string>]> => {
  const out = new Map<string, Array<string>>();
  for (const [k, v] of self) {
    const values = out.get(k);
    if (values === undefined) out.set(k, [v]);
    else values.push(v);
  }
  return [...out];
};

export const isRedactedName = (name: string, names: ReadonlyArray<string>): boolean => {
  const lower = name.toLowerCase();
  return names.some((n) => n.toLowerCase() === lower);
};

const PLAIN = /*#__PURE__*/ tag("Http.Headers.Redaction.Plain");
const SECRET = /*#__PURE__*/ tag("Http.Headers.Redaction.Secret");

export const flickRedact = (names: ReadonlyArray<string>, self: UrlParams) =>
  self.map(([k, v]) => [k, isRedactedName(k, names) ? { _tag: SECRET, _0: redactedMake(v) } : { _tag: PLAIN, _0: v }] as const);

// Effect's patterns: a scheme with an authority, or `//` of a relative URL
const authorityStart = /^(?:(https?|ftp|wss?):[/\\]*|[a-z][a-z\d+.-]*:\/\/|[/\\]{2,})/i;
const specialAuthorityEnd = /[/\\?#]/;
const authorityEnd = /[/?#]/;

/** The URL with its user info, as a URL parser finds it, replaced by `REDACTED:REDACTED`. */
const redactUserinfo = (url: string): string => {
  // a parser skips tabs and newlines anywhere and leading controls and spaces
  let normalized = "";
  const offsets: Array<number> = [];
  for (let i = 0; i < url.length; i++) {
    const code = url.charCodeAt(i);
    if (code === 9 || code === 10 || code === 13 || (normalized === "" && code <= 32)) continue;
    normalized += url[i];
    offsets.push(i);
  }
  const authority = authorityStart.exec(normalized);
  if (authority === null) return url;
  const start = authority[0].length;
  const special = authority[1] !== undefined || !authority[0].includes(":");
  const rest = normalized.slice(start);
  const end = rest.search(special ? specialAuthorityEnd : authorityEnd);
  const userinfoEnd = (end === -1 ? rest : rest.slice(0, end)).lastIndexOf("@");
  return userinfoEnd === -1 ? url : url.slice(0, offsets[start]) + "REDACTED:REDACTED" + url.slice(offsets[start + userinfoEnd]);
};

const decodeQueryKey = (key: string): string => {
  if (!key.includes("%") && !key.includes("+")) return key;
  try {
    return decodeURIComponent(key.replace(/\+/g, " "));
  } catch {
    return key;
  }
};

const redactQuery = (query: string, names: ReadonlyArray<string>): string =>
  query
    .split("&")
    .map((part) => {
      const index = part.indexOf("=");
      return index !== -1 && isRedactedName(decodeQueryKey(part.slice(0, index)), names) ? part.slice(0, index + 1) + "REDACTED" : part;
    })
    .join("&");

export const flickRedactUrl = (names: ReadonlyArray<string>, url: string): string => {
  const masked = url.includes("@") ? redactUserinfo(url) : url;
  const queryStart = masked.indexOf("?");
  if (queryStart === -1 || names.length === 0) return masked;
  const hashStart = masked.indexOf("#");
  if (hashStart !== -1 && hashStart < queryStart) return masked;
  const queryEnd = hashStart === -1 ? masked.length : hashStart;
  return masked.slice(0, queryStart + 1) + redactQuery(masked.slice(queryStart + 1, queryEnd), names) + masked.slice(queryEnd);
};

export const flickDefaultRedactedNames: ReadonlyArray<string> = /*#__PURE__*/ Object.freeze([
  "X-Amz-Signature",
  "X-Amz-Credential",
  "X-Amz-Security-Token",
  "AWSAccessKeyId",
  "Signature",
  "sig",
  "X-Goog-Signature",
]);

const NAMES: object = /*#__PURE__*/ Object.freeze({ key: "effect/http/UrlParams/CurrentRedactedNames" });

/** The names `with_redacted_names` set for what runs now, or the default ones. */
export const redactedNames = (): ReadonlyArray<string> => {
  const names = lookup(NAMES);
  return Array.isArray(names) ? names : flickDefaultRedactedNames;
};

export const flickWithRedactedNames = (names: ReadonlyArray<string>, task: () => unknown): Under =>
  new Under(extend(currentCtx(), NAMES, names), task);

export const flickToList = (self: UrlParams): UrlParams => self;

/** A key's value as a string, or as strings for a key given more than once. */
export const flickToUnknown = (self: UrlParams): unknown =>
  Object.freeze(Object.fromEntries(flickToRecord(self).map(([k, vs]) => [k, vs.length === 1 ? vs[0] : Object.freeze(vs)])));

export const Equivalence = (a: UrlParams, b: UrlParams): boolean =>
  a.length === b.length && a.every(([k, v], i) => k === b[i]?.[0] && v === b[i]?.[1]);

/** `show`: the values of the default secret names masked, whatever `with_redacted_names` set. */
export const flickShow = (self: UrlParams): string =>
  `UrlParams [${self
    .map(([k, v]) => `(${JSON.stringify(k)}, ${JSON.stringify(isRedactedName(k, flickDefaultRedactedNames) ? "<redacted>" : v)})`)
    .join(", ")}]`;
