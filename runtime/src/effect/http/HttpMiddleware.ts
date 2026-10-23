// Effect's `HttpMiddleware` and `HttpEffect` for the bindings of the
// standard library: middlewares of Effect's apps as functions of Flick
// handlers.
import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import { Effect, Layer, Option } from "effect";
import type * as ByteSize from "effect/ByteSize";
import * as HttpEffect from "effect/http/HttpEffect";
import * as HttpPlatform from "effect/http/HttpPlatform";
import * as Middleware from "effect/http/HttpMiddleware";
import { HttpServerRequest } from "effect/http/HttpServerRequest";
import type { HttpServerResponse } from "effect/http/HttpServerResponse";
import * as Response from "effect/http/HttpServerResponse";

import type { Data } from "../../data.ts";
import { type Handler, lift, toApp, toHandler } from "./handler.ts";
import { compression } from "./HttpPlatform.ts";

export * from "effect/http/HttpMiddleware";

export const flickLogger = lift(Middleware.logger);
export const flickWithLoggerDisabled = lift(Middleware.withLoggerDisabled);
export const flickTracer = lift(Middleware.tracer);
export const flickXForwardedHeaders = lift(Middleware.xForwardedHeaders);
export const flickSearchParamsParser = lift(Middleware.searchParamsParser);

/** `Http.Middleware.CorsOptions`. */
export interface CorsOptions {
  readonly allowedOrigins: ReadonlyArray<string>;
  readonly allowedMethods: ReadonlyArray<string>;
  readonly allowedHeaders: ReadonlyArray<string>;
  readonly exposedHeaders: ReadonlyArray<string>;
  readonly maxAge: Option.Option<number>;
  readonly credentials: boolean;
}

const nonEmpty = <A>(xs: ReadonlyArray<A>): ReadonlyArray<A> | undefined => (xs.length === 0 ? undefined : xs);

/** Effect's CORS options of the Flick ones. */
export const corsOptions = (o: CorsOptions) => ({
  allowedOrigins: nonEmpty(o.allowedOrigins),
  allowedMethods: nonEmpty(o.allowedMethods),
  allowedHeaders: nonEmpty(o.allowedHeaders),
  exposedHeaders: nonEmpty(o.exposedHeaders),
  maxAge: Option.getOrUndefined(o.maxAge),
  credentials: o.credentials,
});

export const flickCors = (o: CorsOptions, handler: Handler) => lift(Middleware.cors(corsOptions(o)))(handler);

/** `Http.Middleware.CompressionOptions`. */
export interface CompressionOptions {
  readonly algorithms: ReadonlyArray<Data>;
  readonly minSize: Option.Option<number>;
  readonly levels: ReadonlyArray<readonly [Data, number]>;
}

const compressWith =
  (platform: Layer.Layer<HttpPlatform.HttpPlatform>) => (o: CompressionOptions, handler: Handler) =>
    lift((app) =>
      Effect.provide(
        Middleware.compression({
          algorithms: o.algorithms.length === 0 ? undefined : o.algorithms.map(compression.js),
          minSize: Option.getOrUndefined(o.minSize),
          levels: Object.fromEntries(o.levels.map(([algorithm, level]) => [compression.js(algorithm), level])),
        })(app),
        platform,
      ),
    )(handler);

export const flickCompression = compressWith(NodeHttpServer.layerHttpServices);

const web = HttpPlatform.makeCompressionWeb({
  algorithms: ["gzip", "deflate"],
  transform: (algorithm) => HttpPlatform.compressionTransformWeb(algorithm),
});

/** The web compression with the headers `HttpPlatform.make` adds to each compressed response. */
const webCompression: HttpPlatform.Compression = {
  algorithms: web.algorithms,
  compressResponse: (response, algorithm, o) =>
    Effect.map(web.compressResponse(response, algorithm, o), (compressed) => {
      if (compressed === response) return response;
      const etag = compressed.headers["etag"];
      const vary = compressed.headers["vary"];
      return Response.setHeaders(compressed, {
        "content-encoding": algorithm,
        vary: vary === undefined ? "Accept-Encoding" : `${vary}, Accept-Encoding`,
        ...(etag !== undefined && !etag.startsWith("W/") ? { etag: `W/${etag}` } : {}),
      });
    }),
};

const webPlatform = Layer.effect(
  HttpPlatform.HttpPlatform,
  Effect.map(Effect.service(HttpPlatform.HttpPlatform), (p) => HttpPlatform.HttpPlatform.of({ ...p, compression: webCompression })),
).pipe(Layer.provide(NodeHttpServer.layerHttpServices));

export const flickCompressionWeb = compressWith(webPlatform);

export const flickDecompression = (max: Option.Option<ByteSize.ByteSize>, handler: Handler) =>
  lift(Middleware.decompression({ maxDecompressedSize: Option.getOrUndefined(max) }))(handler);

export const flickWithTracerDisabledWhen = <A, E, R>(
  f: (request: HttpServerRequest) => boolean,
  effect: Effect.Effect<A, E, R>,
) => Effect.provideService(effect, Middleware.TracerDisabledWhen, f);

export const flickWithTracerDisabledForUrls = <A, E, R>(urls: ReadonlyArray<string>, effect: Effect.Effect<A, E, R>) =>
  Effect.provide(effect, Middleware.layerTracerDisabledForUrls(urls));

export const flickWithSpanNameGenerator = <A, E, R>(
  f: (request: HttpServerRequest) => string,
  effect: Effect.Effect<A, E, R>,
) => Effect.provideService(effect, Middleware.SpanNameGenerator, f);

export const flickWithTracerHeaderFilter = <A, E, R>(
  f: (name: string, phase: "request" | "response") => boolean,
  effect: Effect.Effect<A, E, R>,
) => Effect.provideService(effect, Middleware.TracerHeaderFilter, f);

type PreResponse = HttpEffect.PreResponseHandler;

export const flickAppendPreResponseHandler = (f: PreResponse, request: HttpServerRequest) =>
  Effect.provideService(HttpEffect.appendPreResponseHandler(f), HttpServerRequest, request);

export const flickWithPreResponseHandler = (f: PreResponse, handler: Handler) =>
  lift((app) => HttpEffect.withPreResponseHandler(app, f))(handler);

export const flickToHandled = (
  send: (request: HttpServerRequest, response: HttpServerResponse) => Effect.Effect<unknown, unknown, unknown>,
  handler: Handler,
) => toHandler(HttpEffect.toHandled(toApp(handler), send));
