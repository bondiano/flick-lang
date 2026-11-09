// Effect's `HttpRouter` for the bindings of the standard library: a route's
// Flick handler gets the Flick service `Http.Router.RouteContext` of
// Effect's, and a router of routes is a Flick handler.
import { Effect, Option } from "effect";
import * as Router from "effect/http/HttpRouter";
import { HttpServerRequest } from "effect/http/HttpServerRequest";

import type { Data } from "../../data.ts";
import { service } from "../../effects.ts";
import { type AnySchema, codecDecode } from "../../schema/index.ts";
import { method } from "./convert.ts";
import { type Handler, toHandler } from "./handler.ts";
import { flickToUnknown as headersToUnknown } from "./Headers.ts";
import { mapError } from "./HttpServerError.ts";
import { flickJson, flickSearchParams } from "./HttpServerRequest.ts";
import { flickToUnknown, record } from "./UrlParams.ts";

export * from "effect/http/HttpRouter";

type Pairs = ReadonlyArray<readonly [string, string]>;

interface RouteContextImpl {
  readonly params: (unit: undefined) => Effect.Effect<Pairs>;
}

/** The service key of `Http.Router.RouteContext`. */
const FlickRouteContext = /*#__PURE__*/ service<RouteContextImpl>("Http.Router.RouteContext");

const pairs = (params: Readonly<Record<string, string | undefined>>): Pairs =>
  Object.entries(params).flatMap(([k, v]) => (v === undefined ? [] : [[k, v] as const]));

/** `Http.Router.route method path handler`. */
export const flickRoute = (m: Option.Option<Data>, path: Router.PathInput, handler: Handler) =>
  Router.route(
    Option.match(m, { onNone: () => "*" as const, onSome: method.js }),
    path,
    Effect.flatMap(Effect.service(Router.RouteContext), (context) =>
      Effect.flatMap(Effect.service(HttpServerRequest), (request) =>
        Effect.provideService(handler(request), FlickRouteContext, {
          params: () => Effect.succeed(pairs(context.params)),
        }),
      ),
    ),
  );

/** `Http.Router.RouterConfig`. */
export interface RouterConfig {
  readonly ignoreTrailingSlash: Option.Option<boolean>;
  readonly ignoreDuplicateSlashes: Option.Option<boolean>;
  readonly caseSensitive: Option.Option<boolean>;
  readonly maxParamLength: Option.Option<number>;
}

const routerConfig = (c: RouterConfig) =>
  Object.fromEntries(
    Object.entries(c).flatMap(([k, v]: [string, Option.Option<unknown>]) =>
      Option.isSome(v) ? [[k, v.value]] : [],
    ),
  );

/** `Http.Router.router config routes`: one Flick handler of the routes. */
export const flickRouter = (config: RouterConfig, routes: ReadonlyArray<Router.Route<unknown, unknown>>) =>
  Effect.gen(function* () {
    const router = yield* Effect.provideService(Router.make, Router.RouterConfig, routerConfig(config));
    yield* router.addAll(routes);
    return toHandler(mapError(router.asHttpEffect()));
  });

export const flickParamsUnknown = (params: Pairs): unknown => Object.freeze(Object.fromEntries(params));

const routeParams = /*#__PURE__*/ Effect.flatMap(Effect.service(FlickRouteContext), (context) => context.params(undefined));

export const flickSchemaParams = (codec: AnySchema, request: HttpServerRequest) =>
  Effect.flatMap(routeParams, (params) =>
    codecDecode(
      codec,
      Object.freeze({
        ...record(flickSearchParams(request)),
        ...Object.fromEntries(params),
      }),
    ),
  );

const requestInput = (request: HttpServerRequest, params: Pairs) => ({
  method: request.method,
  url: request.url,
  cookies: Object.freeze({ ...request.cookies }),
  headers: headersToUnknown(request.headers),
  pathParams: Object.freeze(Object.fromEntries(params)),
  searchParams: flickToUnknown(flickSearchParams(request)),
});

export const flickSchemaNoBody = (codec: AnySchema, request: HttpServerRequest) =>
  Effect.flatMap(routeParams, (params) => codecDecode(codec, Object.freeze(requestInput(request, params))));

export const flickSchemaJson = (codec: AnySchema, request: HttpServerRequest) =>
  Effect.flatMap(routeParams, (params) =>
    Effect.flatMap(flickJson(request), (body) =>
      codecDecode(codec, Object.freeze({ ...requestInput(request, params), body })),
    ),
  );
