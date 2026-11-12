// `Http.Url` as the platform's `URL`, copied by each setter so that a value
// never changes, as Effect's `Url` does; `UrlError` carries the message of
// the parser's failure.
import { type Option, type Result, err, ok } from "../../../core/data.ts";
import { tag } from "../../../tags.ts";
import { type Redacted, redactedReveal } from "../../redacted.ts";
import { type UrlParams, flickRedactUrl, redactedNames, toString } from "./UrlParams.ts";

const URL_ERROR = /*#__PURE__*/ tag("Http.Url.UrlError.UrlError");

type UrlError = Readonly<{ _tag: string; _0: string }>;

const urlError = (message: string): Result<never, UrlError> => err({ _tag: URL_ERROR, _0: message });

/** `Http.Url.from_string`: Effect's message names the text. */
export const flickFromString = (url: string): Result<URL, UrlError> => {
  try {
    return ok(new URL(url));
  } catch {
    return urlError(`Invalid URL: "${url}"`);
  }
};

/** A browser page's own URL, which a relative URL is resolved against, as Effect's `Url.make` does. */
const baseUrl = (): string | undefined => {
  // only a page has a `location`, though the DOM types always have it
  const location: Partial<Location> | undefined = "location" in globalThis ? globalThis.location : undefined;
  if (location?.origin === undefined || location.pathname === undefined) return undefined;
  const base = location.origin + location.pathname;
  return URL.canParse(base) ? new URL(base).href : undefined;
};

/** `Http.Url.make url params hash`: the parser's own message on failure. */
export const flickMake = (url: string, params: UrlParams, hash: Option<string>): Result<URL, UrlError> => {
  let made: URL;
  try {
    made = new URL(url, baseUrl());
  } catch (cause) {
    return urlError(cause instanceof Error ? cause.message : String(cause));
  }
  for (const [k, v] of params) made.searchParams.append(k, v);
  if (hash._tag === "Some") made.hash = hash.value;
  return ok(made);
};

export const flickHref = (url: URL): string => url.href;

/** A copy of the URL with `change` made to it. */
const mutate = (url: URL, change: (copy: URL) => void): URL => {
  const copy = new URL(url);
  change(copy);
  return copy;
};

type Part = "hash" | "host" | "hostname" | "pathname" | "protocol" | "search" | "username";

const setter = (part: Part) => (value: string) => (url: URL): URL =>
  mutate(url, (copy) => {
    copy[part] = value;
  });

export const setHash = /*#__PURE__*/ setter("hash");
export const setHost = /*#__PURE__*/ setter("host");
export const setHostname = /*#__PURE__*/ setter("hostname");
export const setPathname = /*#__PURE__*/ setter("pathname");
export const setProtocol = /*#__PURE__*/ setter("protocol");
export const setSearch = /*#__PURE__*/ setter("search");
export const setUsername = /*#__PURE__*/ setter("username");

/** `Http.Url.set_href`: a text that is no URL keeps the URL. */
export const flickSetHref = (href: string, url: URL): URL =>
  URL.canParse(href)
    ? mutate(url, (copy) => {
      copy.href = href;
    })
    : url;

export const setPassword = (password: Redacted<string>) => (url: URL): URL =>
  mutate(url, (copy) => {
    copy.password = redactedReveal(password);
  });

// the port setter reads the number as text, as Effect assigns it
export const setPort = (port: number) => (url: URL): URL =>
  mutate(url, (copy) => {
    copy.port = String(port);
  });

export const setUrlParams = (params: UrlParams) => (url: URL): URL =>
  mutate(url, (copy) => {
    copy.search = toString(params);
  });

export const urlParams = (url: URL): UrlParams => [...url.searchParams];

export const modifyUrlParams = (f: (params: UrlParams) => UrlParams) => (url: URL): URL =>
  mutate(url, (copy) => {
    copy.search = toString(f([...copy.searchParams]));
  });

/** `show`: the password and the values of the secret parameters masked. */
export const flickShow = (url: URL): string => flickRedactUrl(redactedNames(), url.href);
