// Effect's `Url` for the bindings of the standard library, with its errors
// as the Flick error `Http.Url.UrlError`.
import { Option, Result } from "effect";
import * as Url from "effect/http/Url";
import * as UrlParams from "effect/http/UrlParams";

import { tag } from "../../tags.ts";

export * from "effect/http/Url";

const URL_ERROR = /*#__PURE__*/ tag("Http.Url.UrlError.UrlError");

const message = (cause: unknown): string => (cause instanceof Error ? cause.message : String(cause));

/** The Flick `UrlError` of a parse failure. */
export const urlError = (cause: unknown) => ({ _tag: URL_ERROR, _0: message(cause) });

const unwrap = (error: { readonly cause?: unknown; readonly message: string }) => urlError(error.cause ?? error.message);

/** `Http.Url.from_string`. */
export const flickFromString = (url: string) => Result.mapError(Url.fromString(url), unwrap);

/** `Http.Url.make url params hash`. */
export const flickMake = (url: string, params: UrlParams.UrlParams, hash: Option.Option<string>) =>
  Result.mapError(Url.make(url, params, Option.getOrUndefined(hash)), unwrap);

/** The text of the URL. */
export const flickHref = (url: URL): string => url.href;

/** `show` of a URL: its password and signature parameters masked. */
export const flickShow = (url: URL): string => UrlParams.redactUrl(url.href);

/** `Http.Url.set_href`: a text that is no URL keeps the URL. */
export const flickSetHref = (href: string, url: URL): URL => (URL.canParse(href) ? Url.setHref(url, href) : url);
