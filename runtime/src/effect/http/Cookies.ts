// Effect's `Cookies` for the bindings of the standard library: cookie
// values cross as `Redacted` strings, attributes as a record of `Option`
// fields, and `CookiesError` as the Flick error `Http.Cookies.CookiesError`.
import { DateTime, Effect, Option, Redacted, type Result } from "effect";
import * as Cookies from "effect/http/Cookies";

import type { Data } from "../../data.ts";
import { tag } from "../../tags.ts";
import { enumeration } from "./convert.ts";

export * from "effect/http/Cookies";

type Options = NonNullable<Cookies.Cookie["options"]>;

const sameSite = enumeration<NonNullable<Options["sameSite"]>>("Http.Cookies.SameSite", [
  ["Lax", "lax"],
  ["Strict", "strict"],
  ["NoSameSite", "none"],
]);

const priority = enumeration<NonNullable<Options["priority"]>>("Http.Cookies.Priority", [
  ["Low", "low"],
  ["Medium", "medium"],
  ["High", "high"],
]);

const reason = enumeration<Cookies.CookiesErrorReason["_tag"]>("Http.Cookies.CookiesErrorReason", [
  ["InvalidCookieName", "InvalidCookieName"],
  ["InvalidCookieValue", "InvalidCookieValue"],
  ["InvalidCookieDomain", "InvalidCookieDomain"],
  ["InvalidCookiePath", "InvalidCookiePath"],
  ["CookieInfinityMaxAge", "CookieInfinityMaxAge"],
]);

/** `Http.Cookies.Attributes` as Flick code builds it. */
export interface Attributes {
  readonly domain: Option.Option<string>;
  readonly expires: Option.Option<DateTime.DateTime>;
  readonly maxAge: Option.Option<Options["maxAge"] & object>;
  readonly path: Option.Option<string>;
  readonly priority: Option.Option<Data>;
  readonly httpOnly: boolean;
  readonly secure: boolean;
  readonly partitioned: boolean;
  readonly sameSite: Option.Option<Data>;
}

/** Effect's cookie options of the attributes. */
export const options = (a: Attributes): Options => ({
  domain: Option.getOrUndefined(a.domain),
  expires: Option.getOrUndefined(Option.map(a.expires, DateTime.toDate)),
  maxAge: Option.getOrUndefined(a.maxAge),
  path: Option.getOrUndefined(a.path),
  priority: Option.getOrUndefined(Option.map(a.priority, priority.js)),
  httpOnly: a.httpOnly || undefined,
  secure: a.secure || undefined,
  partitioned: a.partitioned || undefined,
  sameSite: Option.getOrUndefined(Option.map(a.sameSite, sameSite.js)),
});

const ERROR = tag("Http.Cookies.CookiesError.CookiesError");

/** The Flick `CookiesError` of Effect's. */
export const toFlick = (error: Cookies.CookiesError) => ({ _tag: ERROR, _0: reason.flick(error.reason._tag) });

/** A `Result` of Effect's cookies as an effect failing with the Flick error. */
export const fromResult = <A>(result: Result.Result<A, Cookies.CookiesError>) =>
  Effect.mapError(Effect.fromResult(result), toFlick);

/** `Http.Cookies.make_cookie name value attributes`. */
export const flickMakeCookie = (name: string, value: Redacted.Redacted, a: Attributes) =>
  Effect.suspend(() => fromResult(Cookies.makeCookie(name, Redacted.value(value), options(a))));

/** `Http.Cookies.from_readonly_record pairs`. */
export const flickFromReadonlyRecord = (pairs: ReadonlyArray<readonly [string, Cookies.Cookie]>) =>
  Cookies.fromReadonlyRecord(Object.fromEntries(pairs));

/** `Http.Cookies.get_value name cookies`. */
export const flickGetValue = (name: string, cookies: Cookies.Cookies) =>
  Option.map(Cookies.getValue(cookies, name), Redacted.make);

/** `Http.Cookies.set name value attributes cookies`. */
export const flickSet = (name: string, value: Redacted.Redacted, a: Attributes, cookies: Cookies.Cookies) =>
  Effect.suspend(() => fromResult(Cookies.set(cookies, name, Redacted.value(value), options(a))));

/** `Http.Cookies.set_all entries cookies`. */
export const flickSetAll = (
  entries: ReadonlyArray<readonly [string, Redacted.Redacted, Attributes]>,
  cookies: Cookies.Cookies,
) =>
  Effect.suspend(() =>
    fromResult(Cookies.setAll(cookies, entries.map(([n, v, a]) => [n, Redacted.value(v), options(a)] as const))),
  );

const withoutExpiry = (a: Attributes) => {
  const { expires: _e, maxAge: _m, ...rest } = options(a);
  return rest;
};

/** `Http.Cookies.expire_cookie name attributes cookies`. */
export const flickExpireCookie = (name: string, a: Attributes, cookies: Cookies.Cookies) =>
  Effect.suspend(() => fromResult(Cookies.expireCookie(cookies, name, withoutExpiry(a))));

/** Effect's cookie options without `expires` and `maxAge`. */
export const expireOptions = withoutExpiry;

/** `Http.Cookies.to_record`. */
export const flickToRecord = (cookies: Cookies.Cookies) =>
  Object.entries(Cookies.toRecord(cookies)).map(([k, v]) => [k, Redacted.make(v)] as const);

/** `Http.Cookies.parse_header`. */
export const flickParseHeader = (header: string) =>
  Object.entries(Cookies.parseHeader(header)).map(([k, v]) => [k, Redacted.make(v)] as const);

export const flickName = (cookie: Cookies.Cookie): string => cookie.name;

export const flickValue = (cookie: Cookies.Cookie): Redacted.Redacted => Redacted.make(cookie.value);

/** `show` of a cookie: its `Set-Cookie` form with the value masked. */
export const flickShowCookie = (cookie: Cookies.Cookie): string => {
  const serialized = Cookies.serializeCookie(cookie);
  const attributes = serialized.slice(serialized.indexOf(";") < 0 ? serialized.length : serialized.indexOf(";"));
  return `Cookie ${JSON.stringify(`${cookie.name}=<redacted>${attributes}`)}`;
};

/** `show` of cookies, every value masked. */
export const flickShowCookies = (cookies: Cookies.Cookies): string =>
  `Cookies [${Object.values(cookies.cookies).map(flickShowCookie).join(", ")}]`;
