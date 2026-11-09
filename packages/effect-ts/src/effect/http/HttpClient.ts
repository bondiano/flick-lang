// Effect's `HttpClient` for the bindings of the standard library. A Flick
// client fails with Flick errors only: `flickClient` maps Effect's
// `HttpClientError` of a client at its end, and the combinators that add
// such errors map them again.
import { type Context, Effect, Layer } from "effect";
import * as Cookies from "effect/http/Cookies";
import * as HttpClient from "effect/http/HttpClient";
import * as Request from "effect/http/HttpClientRequest";

import { type Cell, refGet, refSet } from "../../builtins.ts";
import type { Data } from "../../data.ts";
import { tag } from "@flick-lang/runtime/tags";
import { enumeration } from "./convert.ts";
import { mapError } from "./HttpClientError.ts";

export * from "effect/http/HttpClient";

type With<E, R> = HttpClient.HttpClient.With<E, R>;

/** A client of Effect whose `HttpClientError`s are Flick values. */
export const flickClient = (client: HttpClient.HttpClient) => HttpClient.transformResponse(client, mapError);

/** `Http.Client.execute_with client request`. */
export const flickExecute = <E, R>(client: With<E, R>, request: Request.HttpClientRequest) => client.execute(request);

export const flickFilterStatus = <E, R>(f: (status: number) => boolean, client: With<E, R>) =>
  HttpClient.transformResponse(HttpClient.filterStatus(client, f), mapError);

export const flickFilterStatusOk = <E, R>(client: With<E, R>) =>
  HttpClient.transformResponse(HttpClient.filterStatusOk(client), mapError);

const retryOn = /*#__PURE__*/ enumeration<"errors-only" | "response-only" | "errors-and-responses">("Http.Client.RetryOn", [
  ["ErrorsOnly", "errors-only"],
  ["ResponseOnly", "response-only"],
  ["ErrorsAndResponses", "errors-and-responses"],
]);

const ERROR = /*#__PURE__*/ tag("Http.ClientError.HttpClientError.HttpClientError");
const TRANSPORT = /*#__PURE__*/ tag("Http.ClientError.ClientErrorReason.TransportError");
const STATUS = /*#__PURE__*/ tag("Http.ClientError.ClientErrorReason.StatusCodeError");
const TRANSIENT = /*#__PURE__*/ new Set([408, 429, 500, 502, 503, 504]);

interface FlickError {
  readonly _tag: string;
  readonly _0: { readonly _tag: string; readonly _0: { readonly status?: number } };
}

const isFlickError = (e: unknown): e is FlickError =>
  typeof e === "object" && e !== null && "_tag" in e && e._tag === ERROR;

/** A transport failure or a transient status, in the Flick form of the error. */
const isTransient = (e: unknown): boolean =>
  isFlickError(e) &&
  (e._0._tag === TRANSPORT || (e._0._tag === STATUS && TRANSIENT.has(e._0._0.status ?? 0)));

/** `Http.Client.retry_transient on times client`. */
export const flickRetryTransient = <E, R>(on: Data, times: number, client: With<E, R>) =>
  HttpClient.retryTransient(client, { retryOn: retryOn.js(on), times, while: isTransient });

/** `Http.Client.with_cookies_ref cell client`: the cell holds the cookies. */
export const flickWithCookiesRef = <E, R>(cell: Cell<Cookies.Cookies>, client: With<E, R>) =>
  HttpClient.tap(
    HttpClient.mapRequestInputEffect(client, (request) =>
      Effect.sync(() => {
        const header = Cookies.toCookieHeader(refGet(cell));
        return header === "" ? request : Request.setHeader(request, "cookie", header);
      }),
    ),
    (response) => Effect.sync(() => refSet(cell, Cookies.merge(refGet(cell), response.cookies))),
  );

const withReference =
  <A>(reference: Context.Reference<A>) =>
  <B, E, R>(value: A, effect: Effect.Effect<B, E, R>) =>
    Effect.provideService(effect, reference, value);

export const flickWithTracerDisabledWhen = /*#__PURE__*/ withReference(HttpClient.TracerDisabledWhen);
export const flickWithTracerHeaderFilter = /*#__PURE__*/ withReference(HttpClient.TracerHeaderFilter);
export const flickWithTracerPropagation = /*#__PURE__*/ withReference(HttpClient.TracerPropagationEnabled);
export const flickWithSpanNameGenerator = /*#__PURE__*/ withReference(HttpClient.SpanNameGenerator);

/** The operations of the Flick service `Http.Client.HttpClient`. */
const impl = <E, R>(client: With<E, R>): Impl => ({ client: () => Effect.succeed(client) });

export interface Impl {
  readonly client: (unit: undefined) => Effect.Effect<unknown>;
}

/** `Http.Client.layer task`: the service holding the client `task` makes. */
export const flickLayer = <I, E, R>(key: Context.Key<I, Impl>, effect: Effect.Effect<With<unknown, never>, E, R>) =>
  Layer.effect(key, Effect.map(effect, impl));

/** The service `Http.Client.HttpClient` over the Effect client of `layer`. */
export const serviceOf =
  (layer: Layer.Layer<HttpClient.HttpClient>) =>
  <I>(key: Context.Key<I, Impl>): Layer.Layer<I> =>
    Layer.effect(key, Effect.map(Effect.service(HttpClient.HttpClient), (c) => impl(flickClient(c)))).pipe(
      Layer.provide(layer),
    );
