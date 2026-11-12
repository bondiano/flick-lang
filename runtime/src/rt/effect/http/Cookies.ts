// `Http.Cookies` as Effect's `Cookies` keeps them: cookies by name in a plain
// object, so they come out in the order of its keys, names that are array
// indices first, and a cookie keeps its value both as given and encoded.
// The checks, the `Set-Cookie` form and the parsers are those of Effect's
// `Cookies`, so a header reads and writes the same on both runtimes.
import { type Data, type Option, none, some } from "../../../core/data.ts";
import { constructorName, tag } from "../../../tags.ts";
import { fail, raise } from "../../cause.ts";
import { Duration, durationHooks, durationZero } from "../../duration.ts";
import { type Redacted, redactedMake, redactedReveal } from "../../redacted.ts";
import { type DateTime, dateTimeHooks } from "../DateTime.ts";

type SameSite = "lax" | "strict" | "none";
type Priority = "low" | "medium" | "high";

/** The attributes of a cookie, as Effect's cookie options have them. */
interface Options {
  readonly domain?: string | undefined;
  readonly expires?: Date | undefined;
  readonly maxAge?: Duration | undefined;
  readonly path?: string | undefined;
  readonly priority?: Priority | undefined;
  readonly httpOnly?: boolean | undefined;
  readonly secure?: boolean | undefined;
  readonly partitioned?: boolean | undefined;
  readonly sameSite?: SameSite | undefined;
}

/** `Http.Cookies.Cookie`. */
export interface Cookie {
  readonly name: string;
  readonly value: string;
  readonly valueEncoded: string;
  readonly options?: Options | undefined;
}

/** `Http.Cookies.Cookies`: each cookie under its name. */
export type Cookies = Readonly<Record<string, Cookie>>;

/** `Http.Cookies.Attributes` as Flick code builds it. */
interface Attributes {
  readonly domain: Option<string>;
  readonly expires: Option<DateTime>;
  readonly maxAge: Option<unknown>;
  readonly path: Option<string>;
  readonly priority: Option<Data>;
  readonly httpOnly: boolean;
  readonly secure: boolean;
  readonly partitioned: boolean;
  readonly sameSite: Option<Data>;
}

const SAME_SITE: Readonly<Record<string, SameSite>> = { Lax: "lax", Strict: "strict", NoSameSite: "none" };
const PRIORITY: Readonly<Record<string, Priority>> = { Low: "low", Medium: "medium", High: "high" };

const value = <A>(option: Option<A>): A | undefined => (option._tag === "Some" ? option.value : undefined);
const variant = <A>(names: Readonly<Record<string, A>>, option: Option<Data>): A | undefined =>
  option._tag === "Some" ? names[constructorName(option.value._tag)] : undefined;

const options = (a: Attributes): Options => ({
  domain: value(a.domain),
  expires: a.expires._tag === "Some" ? dateTimeHooks.toDate(a.expires.value) : undefined,
  maxAge: a.maxAge._tag === "Some" ? durationHooks.native(a.maxAge.value) : undefined,
  path: value(a.path),
  priority: variant(PRIORITY, a.priority),
  httpOnly: a.httpOnly || undefined,
  secure: a.secure || undefined,
  partitioned: a.partitioned || undefined,
  sameSite: variant(SAME_SITE, a.sameSite),
});

type Reason = "InvalidCookieName" | "InvalidCookieValue" | "InvalidCookieDomain" | "InvalidCookiePath" | "CookieInfinityMaxAge";

const ERROR = /*#__PURE__*/ tag("Http.Cookies.CookiesError.CookiesError");

/** The value, or the Flick `CookiesError` of the reason raised. */
const orFail = <A extends object>(made: A | Reason): A =>
  typeof made === "string" ? raise(fail({ _tag: ERROR, _0: { _tag: tag(`Http.Cookies.CookiesErrorReason.${made}`) } })) : made;

const fieldContentRegExp = /^[\t\x20-\x7e\x80-\xff]+$/;
const cookieNameRegExp = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const cookieWhitespaceRegExp = /^[\t ]+|[\t ]+$/g;
const maxAgeRegExp = /^-?[0-9]+$/;
// a domain or a path without `;`
const cookieDomainRegExp = /^[\t\x20-\x3a\x3c-\x7e\x80-\xff]+$/;
const cookiePathRegExp = /^[\x20-\x3a\x3c-\x7e]+$/;

const validate = (name: string, valueEncoded: string, o: Options | undefined): Reason | undefined => {
  if (!cookieNameRegExp.test(name)) return "InvalidCookieName";
  if (valueEncoded && !fieldContentRegExp.test(valueEncoded)) return "InvalidCookieValue";
  if (o?.domain !== undefined && !cookieDomainRegExp.test(o.domain)) return "InvalidCookieDomain";
  if (o?.path !== undefined && !cookiePathRegExp.test(o.path)) return "InvalidCookiePath";
  if (o?.maxAge !== undefined && !Number.isFinite(o.maxAge.millis)) return "CookieInfinityMaxAge";
  return undefined;
};

const makeCookie = (name: string, raw: string, o: Options): Cookie | Reason => {
  const valueEncoded = encodeURIComponent(raw);
  return validate(name, valueEncoded, o) ?? { name, value: raw, valueEncoded, options: o };
};

/** Sets an own property, `__proto__` too, as Effect's records do. */
const assign = <A>(record: Record<string, A>, key: string, item: A): void => {
  if (key === "__proto__") Object.defineProperty(record, key, { value: item, writable: true, enumerable: true, configurable: true });
  else record[key] = item;
};

const tryDecodeURIComponent = (text: string): string => {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
};

/** The cookie of a `Set-Cookie` header value; an attribute it cannot read is left out. */
const parseSetCookie = (header: string): Cookie | undefined => {
  const parts = header.split(";").map((p) => p.trim()).filter((p) => p !== "");
  const first = parts[0];
  if (first === undefined) return undefined;
  const firstEqual = first.indexOf("=");
  if (firstEqual === -1) return undefined;
  const name = first.slice(0, firstEqual).replace(cookieWhitespaceRegExp, "");
  if (!cookieNameRegExp.test(name)) return undefined;
  const valueEncoded = first.slice(firstEqual + 1).replace(cookieWhitespaceRegExp, "");
  const decoded = tryDecodeURIComponent(valueEncoded);
  if (parts.length === 1) return { name, value: decoded, valueEncoded };
  const o: { -readonly [K in keyof Options]: Options[K] } = {};
  for (const part of parts.slice(1)) {
    const equalIndex = part.indexOf("=");
    const key = equalIndex === -1 ? part : part.slice(0, equalIndex).trim();
    const attribute = equalIndex === -1 ? undefined : part.slice(equalIndex + 1).trim();
    switch (key.toLowerCase()) {
      case "domain": {
        const domain = attribute?.trim().replace(/^\./, "");
        if (domain) o.domain = domain;
        break;
      }
      case "expires": {
        if (attribute === undefined) break;
        const date = new Date(attribute);
        if (!Number.isNaN(date.getTime())) o.expires = date;
        break;
      }
      case "max-age":
        // Effect keeps a negative age as it is
        if (attribute !== undefined && maxAgeRegExp.test(attribute)) o.maxAge = new Duration(Number.parseInt(attribute, 10) * 1000);
        break;
      case "path":
        if (attribute?.[0] === "/") o.path = attribute;
        break;
      case "priority": {
        const priority = attribute?.toLowerCase();
        if (priority === "low" || priority === "medium" || priority === "high") o.priority = priority;
        break;
      }
      case "httponly":
        o.httpOnly = true;
        break;
      case "secure":
        o.secure = true;
        break;
      case "partitioned":
        o.partitioned = true;
        break;
      case "samesite": {
        const sameSite = attribute?.toLowerCase();
        if (sameSite === "lax" || sameSite === "strict" || sameSite === "none") o.sameSite = sameSite;
        break;
      }
    }
  }
  return { name, value: decoded, valueEncoded, options: Object.keys(o).length > 0 ? o : undefined };
};

export const empty: Cookies = /*#__PURE__*/ Object.freeze({});

export const isEmpty = (self: Cookies): boolean => Object.keys(self).length === 0;

export const fromIterable = (cookies: ReadonlyArray<Cookie>): Cookies => {
  const record: Record<string, Cookie> = {};
  for (const cookie of cookies) assign(record, cookie.name, cookie);
  return record;
};

export const fromSetCookie = (headers: ReadonlyArray<string>): Cookies =>
  fromIterable(headers.map((h) => parseSetCookie(h.trim())).filter((c) => c !== undefined));

export const flickFromReadonlyRecord = (pairs: ReadonlyArray<readonly [string, Cookie]>): Cookies => Object.fromEntries(pairs);

export const flickMakeCookie = (name: string, raw: Redacted<string>, a: Attributes): Cookie =>
  orFail(makeCookie(name, redactedReveal(raw), options(a)));

export const setCookie = (cookie: Cookie) => (self: Cookies): Cookies => ({ ...self, [cookie.name]: cookie });

export const setAllCookie = (cookies: ReadonlyArray<Cookie>) => (self: Cookies): Cookies => {
  const record = { ...self };
  for (const cookie of cookies) assign(record, cookie.name, cookie);
  return record;
};

/** `merge that self`: the cookies of `that` over those of `self`. */
export const merge = (that: Cookies) => (self: Cookies): Cookies => ({ ...self, ...that });

export const remove = (name: string) => (self: Cookies): Cookies => Object.fromEntries(Object.entries(self).filter(([k]) => k !== name));

const find = (name: string, self: Cookies): Cookie | undefined => (Object.hasOwn(self, name) ? self[name] : undefined);

export const get = (name: string) => (self: Cookies): Option<Cookie> => {
  const cookie = find(name, self);
  return cookie === undefined ? none : some(cookie);
};

export const flickGetValue = (name: string, self: Cookies): Option<Redacted<string>> => {
  const cookie = find(name, self);
  return cookie === undefined ? none : some(redactedMake(cookie.value));
};

const setOne = (self: Cookies, name: string, raw: string, o: Options): Cookies => {
  const cookie = orFail(makeCookie(name, raw, o));
  return { ...self, [name]: cookie };
};

export const flickSet = (name: string, raw: Redacted<string>, a: Attributes, self: Cookies): Cookies =>
  setOne(self, name, redactedReveal(raw), options(a));

export const flickSetAll = (entries: ReadonlyArray<readonly [string, Redacted<string>, Attributes]>, self: Cookies): Cookies => {
  const record = { ...self };
  for (const [name, raw, a] of entries) assign(record, name, orFail(makeCookie(name, redactedReveal(raw), options(a))));
  return record;
};

export const flickExpireCookie = (name: string, a: Attributes, self: Cookies): Cookies =>
  setOne(self, name, "", { ...options(a), maxAge: durationZero, expires: new Date(0) });

// a cookie Effect would not serialize: one read from a header with a value no header may carry
class CookiesError extends Error {
  override readonly name = "CookiesError";
}

/** Seconds as Effect's `Duration.toSeconds` counts them, exact for a duration of nanoseconds. */
const seconds = (d: Duration): number => (d.nanos === undefined ? d.millis / 1000 : Number(d.nanos) / 1e9);

const PRIORITY_TEXT: Readonly<Record<Priority, string>> = { low: "Low", medium: "Medium", high: "High" };
const SAME_SITE_TEXT: Readonly<Record<SameSite, string>> = { lax: "Lax", strict: "Strict", none: "None" };

export const serializeCookie = (self: Cookie): string => {
  const error = validate(self.name, self.valueEncoded, self.options);
  if (error !== undefined) throw new CookiesError(error);
  let str = `${self.name}=${self.valueEncoded}`;
  const o = self.options;
  if (o === undefined) return str;
  if (o.maxAge !== undefined) str += `; Max-Age=${Math.trunc(seconds(o.maxAge))}`;
  if (o.domain !== undefined) str += `; Domain=${o.domain}`;
  if (o.path !== undefined) str += `; Path=${o.path}`;
  if (o.priority !== undefined) str += `; Priority=${PRIORITY_TEXT[o.priority]}`;
  if (o.expires !== undefined) str += `; Expires=${o.expires.toUTCString()}`;
  if (o.httpOnly) str += "; HttpOnly";
  if (o.secure) str += "; Secure";
  if (o.partitioned) str += "; Partitioned";
  if (o.sameSite !== undefined) str += `; SameSite=${SAME_SITE_TEXT[o.sameSite]}`;
  return str;
};

export const toCookieHeader = (self: Cookies): string =>
  Object.values(self).map((c) => `${c.name}=${c.valueEncoded}`).join("; ");

/** Each cookie's name and value, under the cookie's own name, as Effect's `toRecord`. */
export const flickToRecord = (self: Cookies): ReadonlyArray<readonly [string, Redacted<string>]> => {
  const record: Record<string, string> = {};
  for (const cookie of Object.values(self)) assign(record, cookie.name, cookie.value);
  return Object.entries(record).map(([k, v]) => [k, redactedMake(v)] as const);
};

export const toSetCookieHeaders = (self: Cookies): ReadonlyArray<string> => Object.values(self).map(serializeCookie);

/** The names and values of a `Cookie` header; the first of a name is kept. */
export const flickParseHeader = (header: string): ReadonlyArray<readonly [string, Redacted<string>]> => {
  const result: Record<string, string> = {};
  const strLen = header.length;
  let pos = 0;
  let terminatorPos = 0;
  while (terminatorPos !== strLen) {
    terminatorPos = header.indexOf(";", pos);
    if (terminatorPos === -1) terminatorPos = strLen;
    let eqIdx = header.indexOf("=", pos);
    if (eqIdx === -1) break;
    if (eqIdx > terminatorPos) {
      pos = terminatorPos + 1;
      continue;
    }
    const key = header.substring(pos, eqIdx++).trim();
    if (!Object.hasOwn(result, key)) {
      let raw = header.substring(eqIdx, terminatorPos).trim();
      if (raw.charCodeAt(0) === 0x22) raw = raw.slice(1, -1).trim();
      assign(result, key, raw.includes("%") ? tryDecodeURIComponent(raw) : raw);
    }
    pos = terminatorPos + 1;
  }
  return Object.entries(result).map(([k, v]) => [k, redactedMake(v)] as const);
};

export const flickName = (cookie: Cookie): string => cookie.name;

export const flickValue = (cookie: Cookie): Redacted<string> => redactedMake(cookie.value);

/** `show` of a cookie: its `Set-Cookie` form with the value masked. */
export const flickShowCookie = (cookie: Cookie): string => {
  const serialized = serializeCookie(cookie);
  const semicolon = serialized.indexOf(";");
  return `Cookie ${JSON.stringify(`${cookie.name}=<redacted>${semicolon < 0 ? "" : serialized.slice(semicolon)}`)}`;
};

export const flickShowCookies = (self: Cookies): string => `Cookies [${Object.values(self).map(flickShowCookie).join(", ")}]`;
