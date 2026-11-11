// Flick handlers, `(request) => Effect<response>`, and Effect's HTTP apps,
// effects that read the request from their context, each as the other.
import { Effect } from "effect";
import { HttpServerRequest } from "effect/http/HttpServerRequest";
import type { HttpServerResponse } from "effect/http/HttpServerResponse";

/** A Flick handler as JS gets it. */
export type Handler<E = unknown, R = unknown> = (request: HttpServerRequest) => Effect.Effect<HttpServerResponse, E, R>;

/** Effect's app of a Flick handler. */
export const toApp = <E, R>(handler: Handler<E, R>): Effect.Effect<HttpServerResponse, E, R | HttpServerRequest> =>
  Effect.flatMap(Effect.service(HttpServerRequest), (request) => Effect.interruptible(handler(request)));

/** The Flick handler of Effect's app. */
export const toHandler =
  <A, E, R>(app: Effect.Effect<A, E, R>) =>
  (request: HttpServerRequest): Effect.Effect<A, E, Exclude<R, HttpServerRequest>> =>
    // Native HTTP apps enter under HttpEffect's mask; routers restore ordinary
    // routes themselves and preserve explicitly uninterruptible routes.
    Effect.uninterruptible(Effect.provideService(app, HttpServerRequest, request));

/** A middleware of Effect's apps as a function of Flick handlers. */
export const lift =
  (middleware: (app: Effect.Effect<HttpServerResponse, unknown, unknown>) => Effect.Effect<HttpServerResponse, unknown, unknown>) =>
  (handler: Handler): Handler =>
    toHandler(middleware(toApp(handler)));
