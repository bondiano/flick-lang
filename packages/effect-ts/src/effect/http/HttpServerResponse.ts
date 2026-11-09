// Effect's `HttpServerResponse` for the bindings of the standard library:
// options as records of `Option` fields, cookie values as `Redacted`
// strings, and the responses a server answers the Flick errors of `Http` with.
import { Cause, Effect, Option, Redacted, Stream } from "effect";
import type * as Cookies from "effect/http/Cookies";
import type * as ClientRequest from "effect/http/HttpClientRequest";
import * as HttpServerError from "effect/http/HttpServerError";
import * as Response from "effect/http/HttpServerResponse";
import type * as UrlParams from "effect/http/UrlParams";

import { type Failure, failureRethrow } from "../../cause.ts";
import { type AnySchema, codecEncode } from "../../schema/index.ts";
import { defined, toBytes } from "./convert.ts";
import { type Attributes, expireOptions, fromResult, options as cookieOptions, toFlick } from "./Cookies.ts";
import { mapError } from "./HttpBody.ts";
import { formData } from "./HttpBody.ts";
import { statusOf } from "./HttpServerError.ts";
import { multipartStatusOf } from "./Multipart.ts";

export * from "effect/http/HttpServerResponse";

type Pairs = ReadonlyArray<readonly [string, string]>;

/** `Http.ServerResponse.Options`. */
export interface FlickOptions {
  readonly status: Option.Option<number>;
  readonly statusText: Option.Option<string>;
  readonly headers: Option.Option<Pairs>;
  readonly cookies: Option.Option<Cookies.Cookies>;
  readonly contentType: Option.Option<string>;
}

/** Effect's options of a response of the Flick options. */
export const options = (o: FlickOptions): Response.Options.WithContentType => defined(o);

type R = Response.HttpServerResponse;

export const flickEmpty = (o: FlickOptions) => Response.empty(options(o));
export const flickRedirect = (location: string, o: FlickOptions) => Response.redirect(location, options(o));
export const flickUint8Array = (bytes: ReadonlyArray<number>, o: FlickOptions) =>
  Response.uint8Array(toBytes(bytes), options(o));
export const flickText = (body: string, o: FlickOptions) => Response.text(body, options(o));
export const flickStream = (chunks: Stream.Stream<ReadonlyArray<number>, unknown>, o: FlickOptions) =>
  Response.stream(Stream.map(chunks, toBytes), options(o));
export const flickStreamText = (chunks: Stream.Stream<string, unknown>, o: FlickOptions) =>
  Response.stream(Stream.encodeText(chunks), {
    ...options(o),
    contentType: Option.getOrElse(o.contentType, () => "text/plain"),
  });
export const flickJson = (body: unknown, o: FlickOptions) => mapError(Response.json(body, options(o)));
export const flickSchemaJson = (codec: AnySchema, value: unknown, o: FlickOptions) =>
  Effect.flatMap(codecEncode(codec, value), (body) => flickJson(body, o));
export const flickUrlParams = (params: UrlParams.UrlParams, o: FlickOptions) => Response.urlParams(params, options(o));
export const flickFormData = (pairs: Pairs, o: FlickOptions) => Response.formData(formData(pairs), options(o));

export const flickStatus = (self: R): number => self.status;
export const flickStatusText = (self: R): Option.Option<string> => Option.fromUndefinedOr(self.statusText);
export const flickHeaders = (self: R) => self.headers;
export const flickCookies = (self: R) => self.cookies;
export const flickBody = (self: R) => self.body;

export const flickSetCookie = (name: string, value: Redacted.Redacted, a: Attributes, self: R) =>
  Effect.mapError(Response.setCookie(self, name, Redacted.value(value), cookieOptions(a)), toFlick);

export const flickExpireCookie = (name: string, a: Attributes, self: R) =>
  Effect.mapError(Response.expireCookie(self, name, expireOptions(a)), toFlick);

export const flickSetCookies = (
  entries: ReadonlyArray<readonly [string, Redacted.Redacted, Attributes]>,
  self: R,
) =>
  Effect.mapError(
    Response.setCookies(
      self,
      entries.map(([n, v, a]) => [n, Redacted.value(v), cookieOptions(a)] as const),
    ),
    toFlick,
  );

/** A `Result` of Effect's cookies as an effect failing with the Flick error. */
export const cookiesResult = fromResult;

export const flickToClientResponse = (request: ClientRequest.HttpClientRequest, self: R) =>
  Response.toClientResponse(self, { request });

export const flickOmitsBody = (withoutBody: boolean, self: R): boolean => Response.omitsBody(self, withoutBody);

/** The response a server answers a Flick error of `Http` with, if it is one. */
export const respondTo = (error: unknown): R | undefined => {
  const status = statusOf(error) ?? multipartStatusOf(error);
  return status === undefined ? undefined : Response.empty({ status });
};

/** The response of a failure: a Flick error's status, or Effect's answer. */
export const causeResponse = (cause: Cause.Cause<unknown>): Effect.Effect<R> => {
  const reason = cause.reasons.find(Cause.isFailReason);
  const flick = reason === undefined ? undefined : respondTo(reason.error);
  return flick === undefined ? Effect.map(HttpServerError.causeResponse(cause), ([response]) => response) : Effect.succeed(flick);
};

/** `Http.ServerResponse.cause_response failure`. */
export const flickCauseResponse = (failure: Failure) =>
  Effect.catchCause(Effect.as(failureRethrow(failure), Response.empty({ status: 500 })), causeResponse);
