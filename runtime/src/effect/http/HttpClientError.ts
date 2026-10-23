// Effect's `HttpClientError` for the bindings of the standard library, and
// the adapter that turns it into the Flick error `Http.ClientError.HttpClientError`.
import { Effect } from "effect";
import * as HttpClientError from "effect/http/HttpClientError";
import * as UrlParams from "effect/http/UrlParams";

import { tag } from "../../tags.ts";
import { method, option } from "./convert.ts";

export * from "effect/http/HttpClientError";

const ERROR = tag("Http.ClientError.HttpClientError.HttpClientError");
const reasonTag = (name: string): string => tag(`Http.ClientError.ClientErrorReason.${name}`);

const describe = (description: string | undefined, cause: unknown) =>
  option(description ?? (cause instanceof Error ? cause.message : typeof cause === "string" ? cause : undefined));

/** The Flick value of an Effect `HttpClientError`. */
export const toFlick = (error: HttpClientError.HttpClientError) => {
  const reason = error.reason;
  const request = {
    method: method.flick(reason.request.method),
    url: UrlParams.redactUrl(reason.request.url),
  };
  const details =
    "response" in reason
      ? { ...request, status: reason.response.status, description: describe(reason.description, reason.cause) }
      : { ...request, description: describe(reason.description, reason.cause) };
  return { _tag: ERROR, _0: { _tag: reasonTag(reason._tag), _0: details } };
};

/** Typed failures of `effect` that are an Effect `HttpClientError` become the Flick error. */
export const mapError = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
  Effect.mapError(effect, (e) => (HttpClientError.isHttpClientError(e) ? toFlick(e) : e));
