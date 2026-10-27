// `HttpApi.listen`: an API served on Node's HTTP server for as long as the
// Flick scope that started it. Each request is decoded with the endpoint's
// codecs (a failure is 400 with the issues, never the input), runs its
// handler with the services of `listen`, and its typed error is sent with
// the declared status. Closing the scope stops accepting, lets the running
// requests finish and runs their finalizers.
import * as NodeHttpServer from "@effect/platform-node/NodeHttpServer";
import { type Context, Effect, Exit, Option, Result, Schema, Scope } from "effect";
import { HttpRouter, type HttpServerRequest, HttpServerResponse } from "effect/http";
import { javascript as scalarJs } from "effect/http-api/internal/httpApiScalar";
import { css as swaggerCss, javascript as swaggerJs } from "effect/http-api/internal/httpApiSwagger";

import type { Tagged } from "../data.ts";
import { nodeHttp } from "../host.ts";
import { acquireRelease } from "../owner.ts";
import { codecDecode, codecEncode } from "../schema/index.ts";
import type { SchemaError } from "../schema/issues.ts";
import type { AnySchema } from "../schema/schemas.ts";
import { tag } from "../tags.ts";
import { multipartStatusOf } from "../effect/http/Multipart.ts";
import { otherPayload, otherSuccess, unwrapExtras } from "./encodings.ts";
import { type Api, type Endpoint, hasPayload, type Security, typed } from "./model.ts";
import { httpApiOpenApi } from "./openapi.ts";

type Handler = (request: unknown) => Effect.Effect<unknown, unknown, unknown>;

type Handlers = Readonly<Record<string, Readonly<Record<string, Handler>>>>;

export interface ServeOptions {
  readonly host: string;
  readonly port: number;
  readonly openapi: Option.Option<string>;
  readonly swagger: Option.Option<string>;
  readonly scalar: Option.Option<string>;
}

/** `HttpApi.Server s`. */
export interface Server {
  readonly port: number;
  readonly scope: Scope.Closeable;
}

/** `HttpApi.ServeError`. */
export type ServeError = Tagged<string, string>;

const serveError = (message: string): ServeError => ({ _tag: tag("HttpApi.ServeError.ServeError"), _0: message });

const decode = (codec: AnySchema | null, input: unknown, extra: "refuse" | "ignore" = "refuse"): Effect.Effect<unknown, SchemaError, unknown> =>
  codec === null ? Effect.succeed(undefined) : codecDecode(codec, input, extra);

/** 400 with the issues of a request that does not decode, or the status of a form's `MultipartError`. */
const badRequest = (error: unknown): HttpServerResponse.HttpServerResponse => {
  const status = multipartStatusOf(error);
  return status === undefined
    ? HttpServerResponse.jsonUnsafe({ error: "BadRequest", issues: typed<SchemaError>(error)._0 }, { status: 400 })
    : HttpServerResponse.empty({ status });
};

const internal = (): HttpServerResponse.HttpServerResponse =>
  HttpServerResponse.jsonUnsafe({ error: "InternalServerError" }, { status: 500 });

/**
 * The body in the endpoint's encoding: JSON (text that is no JSON stays a
 * string, which a record codec refuses), the text itself, or the fields of
 * a URL-encoded form.
 */
const body = (request: HttpServerRequest.HttpServerRequest, encoding: Endpoint["payloadEncoding"]): Effect.Effect<unknown> =>
  Effect.map(Effect.orElseSucceed(request.text, () => ""), (text) => {
    if (encoding === "text") return text;
    if (encoding === "form") return Object.fromEntries(new URLSearchParams(text));
    try {
      return JSON.parse(text) as unknown;
    } catch {
      return text;
    }
  });

const HTTP_ERROR = /*#__PURE__*/ tag("HttpApi.HttpError.HttpError");

/** `HttpApi.HttpError`, which carries its own status. */
const isHttpError = (error: unknown): error is { readonly _0: { readonly status: number; readonly message: string } } =>
  typeof error === "object" && error !== null && Reflect.get(error, "_tag") === HTTP_ERROR;

/**
 * The response of a typed error: the first declared case whose codec has
 * the value, with its status; an `HttpError` always with its own status,
 * as `{ message }` when no case declares it; a streamed form's
 * `MultipartError` with the status it names; anything else 500.
 */
const failed = (endpoint: Endpoint, error: unknown): Effect.Effect<HttpServerResponse.HttpServerResponse, never, unknown> => {
  const found = endpoint.errors.find((e) => Schema.is(e.codec)(error));
  const own = isHttpError(error) ? error._0 : undefined;
  if (found === undefined) {
    if (own !== undefined) return Effect.succeed(HttpServerResponse.jsonUnsafe({ message: own.message }, { status: own.status }));
    if (multipartStatusOf(error) !== undefined) return Effect.succeed(badRequest(error));
    return Effect.as(Effect.logError(`${endpoint.name}: an undeclared error`), internal());
  }
  return Effect.match(codecEncode(found.codec, error), {
    onFailure: internal,
    onSuccess: (encoded) => HttpServerResponse.jsonUnsafe(encoded, { status: own?.status ?? found.status }),
  });
};

const successBody = (endpoint: Endpoint, value: unknown): Effect.Effect<HttpServerResponse.HttpServerResponse, unknown, unknown> => {
  const other = otherSuccess(endpoint, value);
  if (other !== undefined) return Effect.succeed(other);
  const codec = endpoint.success;
  if (codec === null) return Effect.succeed(HttpServerResponse.empty({ status: endpoint.status }));
  return Effect.map(codecEncode(codec, value), (encoded) =>
    endpoint.successEncoding === "text"
      ? HttpServerResponse.text(String(encoded), { status: endpoint.status })
      : HttpServerResponse.jsonUnsafe(encoded, { status: endpoint.status }),
  );
};

const succeeded = (endpoint: Endpoint, value: unknown): Effect.Effect<HttpServerResponse.HttpServerResponse, never, unknown> =>
  Effect.orElseSucceed(
    Effect.flatMap(unwrapExtras(endpoint.successExtras, value), ([inner, extras]) =>
      Effect.flatMap(successBody(endpoint, inner), extras),
    ),
    internal,
  );

/** A middleware hook's result: the effect of a runtime function, or nothing of a lifted pure one. */
const effectOf = (value: unknown): Effect.Effect<unknown, unknown, unknown> =>
  Effect.isEffect(value) ? value : Effect.void;

/** Whether the request carries the credentials of one of the schemes. */
const authorized = (schemes: ReadonlyArray<Security>, request: HttpServerRequest.HttpServerRequest, url: URL): boolean => {
  const authorization = (request.headers["authorization"] ?? "").toLowerCase();
  const cookies = request.headers["cookie"] ?? "";
  return (
    schemes.length === 0 ||
    schemes.some((s) => {
      if (s.kind === "bearer") return authorization.startsWith("bearer ");
      if (s.kind === "basic") return authorization.startsWith("basic ");
      if (s.in === "query") return url.searchParams.has(s.key);
      if (s.in === "cookie") return cookies.split(";").some((c) => c.trim().startsWith(`${s.key}=`));
      return request.headers[s.key.toLowerCase()] !== undefined;
    })
  );
};

const unauthorized = (): HttpServerResponse.HttpServerResponse =>
  HttpServerResponse.jsonUnsafe({ error: "Unauthorized" }, { status: 401 });

const route =
  (endpoint: Endpoint, handler: Handler, context: Context.Context<unknown>) =>
  (request: HttpServerRequest.HttpServerRequest): Effect.Effect<HttpServerResponse.HttpServerResponse, never, unknown> =>
    Effect.gen(function* () {
      // HEAD has no route of its own: it is served by the `*` route of its path
      if (endpoint.method === "HEAD" && request.method !== "HEAD") return HttpServerResponse.empty({ status: 405 });
      const params = yield* HttpRouter.params;
      const url = new URL(request.url, "http://localhost");
      if (!authorized(endpoint.security, request, url)) return unauthorized();
      const other = ["multipart", "bytes", "parts"].includes(endpoint.payloadEncoding);
      const payload = !hasPayload(endpoint) || other ? undefined : yield* body(request, endpoint.payloadEncoding);
      const decoded = yield* Effect.result(
        Effect.all({
          path: decode(endpoint.params, params),
          query: decode(endpoint.query, Object.fromEntries(url.searchParams)),
          // a request has more headers than any endpoint declares
          headers: decode(endpoint.headers, request.headers, "ignore"),
          payload: other ? otherPayload(endpoint, request) : decode(endpoint.payload, payload),
        }),
      );
      if (Result.isFailure(decoded)) return badRequest(decoded.failure);
      const hooks = endpoint.middleware.map((hook) => Effect.asVoid(effectOf(hook(decoded.success))));
      const run = Effect.andThen(Effect.all(hooks, { discard: true }), () => handler(decoded.success));
      return yield* Effect.matchEffect(Effect.provideContext(run, context), {
        onFailure: (error) => failed(endpoint, error),
        onSuccess: (value) => succeeded(endpoint, value),
      });
    });

/** A route's path: it starts with `/`, as Effect's router needs. */
const routePath = (path: string): HttpRouter.PathInput => {
  if (!isRoutePath(path)) throw new TypeError(`an HTTP path starts with "/": ${path}`);
  return path;
};

const isRoutePath = (path: string): path is `/${string}` => path.startsWith("/");

const escapeJson = (text: string): string => text.replaceAll("<", "\\u003c");

const swaggerPage = (title: string, spec: string): string =>
  `<!DOCTYPE html><html lang="en"><head><meta charset="utf-8" /><title>${title}</title><style>${swaggerCss}</style></head><body><div id="swagger-ui"></div><script id="swagger-spec" type="application/json">${escapeJson(spec)}</script><script>${swaggerJs}
window.onload = () => { window.ui = SwaggerUIBundle({ spec: JSON.parse(document.getElementById("swagger-spec").textContent), dom_id: "#swagger-ui" }); };</script></body></html>`;

const scalarPage = (title: string, spec: string): string =>
  `<!doctype html><html><head><meta charset="utf-8" /><title>${title}</title></head><body><div id="api-reference-container"></div><script>${scalarJs}</script><script>window.Scalar.createApiReference(document.getElementById("api-reference-container"), { _integration: "html", content: ${escapeJson(spec)} })</script></body></html>`;

const start = (options: ServeOptions, api: Api, handlers: Handlers) =>
  Effect.gen(function* () {
    const context = yield* Effect.context<unknown>();
    const router = yield* HttpRouter.make;
    for (const group of api.groups) {
      for (const endpoint of group.endpoints) {
        const handler = handlers[group.name]?.[endpoint.name];
        if (handler === undefined) return yield* Effect.die(new TypeError(`no handler for ${group.name}.${endpoint.name}`));
        const method = endpoint.method === "HEAD" ? "*" : endpoint.method;
        yield* router.add(method, routePath(endpoint.path), route(endpoint, handler, context));
      }
    }
    const docs = [options.openapi, options.swagger, options.scalar].some(Option.isSome);
    const spec = httpApiOpenApi(api);
    if (docs && Result.isFailure(spec)) return yield* Effect.fail(serveError(spec.failure._0));
    const text = Result.isSuccess(spec) ? spec.success : "";
    if (Option.isSome(options.openapi)) {
      yield* router.add("GET", routePath(options.openapi.value), HttpServerResponse.text(text, { contentType: "application/json" }));
    }
    if (Option.isSome(options.swagger)) {
      yield* router.add("GET", routePath(options.swagger.value), HttpServerResponse.html(swaggerPage(api.name, text)));
    }
    if (Option.isSome(options.scalar)) {
      yield* router.add("GET", routePath(options.scalar.value), HttpServerResponse.html(scalarPage(api.name, text)));
    }
    const scope = yield* Scope.make();
    const made = yield* NodeHttpServer.make(() => nodeHttp().createServer(), {
      host: options.host,
      port: options.port,
    }).pipe(
      Effect.provideService(Scope.Scope, scope),
      Effect.mapError((error) => serveError(error.message)),
      Effect.onError((cause) => Scope.close(scope, Exit.failCause(cause))),
    );
    yield* made.serve(router.asHttpEffect()).pipe(Effect.provideService(Scope.Scope, scope));
    const port = made.address._tag === "UnixPathAddress" ? 0 : made.address.port;
    return { port, scope } satisfies Server;
  });

/** `HttpApi.listen options api handlers`, owned by the innermost scope. */
export const httpApiListen = (options: ServeOptions, api: Api, handlers: Handlers): Effect.Effect<Server, ServeError, unknown> =>
  acquireRelease(start(options, api, handlers), (server) => Scope.close(server.scope, Exit.void));

export const httpApiPort = (server: Server): number => server.port;
