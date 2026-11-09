// Effect's `HttpServerError` for the bindings of the standard library, and
// the adapters between it and the Flick errors `Http.ServerError.*`.
import { Effect } from "effect";
import * as HttpServerError from "effect/http/HttpServerError";
import * as UrlParams from "effect/http/UrlParams";

import { tag } from "@flick-lang/runtime/tags";
import { method, option } from "./convert.ts";

export * from "effect/http/HttpServerError";

const ERROR = /*#__PURE__*/ tag("Http.ServerError.HttpServerError.HttpServerError");
const SERVE_ERROR = /*#__PURE__*/ tag("Http.ServerError.ServeError.ServeError");
const reasonTag = (name: string): string => tag(`Http.ServerError.ServerErrorReason.${name}`);

const STATUS = /*#__PURE__*/ new Map([
  [reasonTag("RequestParseError"), 400],
  [reasonTag("RouteNotFound"), 404],
  [reasonTag("InternalError"), 500],
  [reasonTag("ResponseError"), 500],
]);

const describe = (description: string | undefined, cause: unknown) =>
  option(description ?? (cause instanceof Error ? cause.message : typeof cause === "string" ? cause : undefined));

/** The Flick value of an Effect `HttpServerError`. */
export const toFlick = (error: HttpServerError.HttpServerError) => {
  const reason = error.reason;
  const request = { method: method.flick(reason.request.method), url: UrlParams.redactUrl(reason.request.url) };
  const details =
    reason._tag === "ResponseError"
      ? { ...request, status: reason.response.status, description: describe(reason.description, reason.cause) }
      : { ...request, description: describe(reason.description, reason.cause) };
  return { _tag: ERROR, _0: { _tag: reasonTag(reason._tag), _0: details } };
};

/** Typed failures of `effect` that are an Effect `HttpServerError` become the Flick error. */
export const mapError = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
  Effect.mapError(effect, (e) => (HttpServerError.isHttpServerError(e) ? toFlick(e) : e));

/** The Flick `ServeError` of Effect's. */
export const serveError = (error: HttpServerError.ServeError) => ({
  _tag: SERVE_ERROR,
  _0: error.cause instanceof Error ? error.cause.message : String(error.cause),
});

interface Tagged {
  readonly _tag: string;
  readonly _0: { readonly _tag: string };
}

const isFlick = (e: unknown): e is Tagged => typeof e === "object" && e !== null && "_tag" in e && e._tag === ERROR;

/** The status of a Flick `HttpServerError`, or `undefined` for any other value. */
export const statusOf = (e: unknown): number | undefined => (isFlick(e) ? STATUS.get(e._0._tag) : undefined);
