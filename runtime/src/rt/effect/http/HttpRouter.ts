// `Http.Router` on Flick's own runtime, as Effect's `HttpRouter` routes:
// paths match with the find-my-way Effect vendors (`../../vendor`), a
// pattern ending in `/*` matches its prefix too, `HEAD` falls back to the
// `GET` routes, and a route under a prefix sees the URL below it. A route's
// handler reads its parameters through the Flick service `RouteContext`,
// and runs interruptible, as Effect runs a route.
//
// A `Route` is a frozen record of the method (`*` for any), the path
// pattern, the handler, and the prefix it was put under.
import type { Option } from "../../../core/data.ts";
import { fail, raise } from "../../cause.ts";
import { codecDecode } from "../../codec.ts";
import { type Run, Under, atMask, currentCtx, extend } from "../../fiber.ts";
import { operation, service } from "../../handle.ts";
import { type ServerRequest, httpServerError, methodFromFlick, withUrl } from "../../http/model.ts";
import { readJson } from "../../http/request.ts";
import type { Witness } from "../../schema.ts";
import * as FindMyWay from "../../vendor/find-my-way/FindMyWay.ts";
import { getClosingParenthensePosition, getMaxParamLengthExceeded } from "../../vendor/find-my-way/router.ts";
import { flickCookies, flickSearchParams } from "./HttpServerRequest.ts";
import { flickToUnknown as headersToUnknown } from "./Headers.ts";
import { flickToRecord, flickToUnknown } from "./UrlParams.ts";
import { redactedReveal } from "../../redacted.ts";

type Pairs = ReadonlyArray<readonly [string, string]>;
type Handler = (request: ServerRequest) => unknown;

/** `Http.Router.Route r e`. */
export interface Route {
  readonly method: string;
  readonly path: string;
  readonly handler: Handler;
  readonly prefix: string | undefined;
}

const ROUTE_CONTEXT = /*#__PURE__*/ service("Http.Router.RouteContext");

const removeTrailingSlash = (path: string): string => (path.endsWith("/") ? path.slice(0, -1) : path);

/** `Http.Router.route method path handler`. */
export const flickRoute = (method: Option<Readonly<{ _tag: string }>>, path: string, handler: Handler): Route =>
  Object.freeze({ method: method._tag === "Some" ? methodFromFlick(method.value) : "*", path, handler, prefix: undefined });

/** `prefix_path prefix path`: `*` becomes a wildcard below the prefix, `/` the prefix itself. */
export const prefixPath = (prefix: string) => (self: string): string => {
  const trimmed = removeTrailingSlash(prefix);
  if (self === "*") return `${trimmed}/*`;
  if (self === "/") return trimmed;
  return trimmed + self;
};

export const prefixRoute = (prefix: string) => (self: Route): Route =>
  Object.freeze({
    ...self,
    path: prefixPath(prefix)(self.path),
    prefix: self.prefix === undefined ? removeTrailingSlash(prefix) : prefixPath(prefix)(self.prefix),
  });

/** `Http.Router.RouterConfig`. */
export interface RouterConfig {
  readonly ignoreTrailingSlash: Option<boolean>;
  readonly ignoreDuplicateSlashes: Option<boolean>;
  readonly caseSensitive: Option<boolean>;
  readonly maxParamLength: Option<number>;
}

/** How many segments of a URL a prefix takes; a slash in a parameter's pattern is not a boundary. */
const countSegments = (prefix: string, ignoreDuplicateSlashes: boolean): number => {
  let count = 0;
  let inParamName = false;
  for (let i = 0; i < prefix.length; i++) {
    const char = prefix[i];
    if (char === "/") {
      if (!ignoreDuplicateSlashes || prefix[i - 1] !== "/") count++;
      inParamName = false;
    } else if (char === ":") {
      if (prefix[i + 1] === ":") i++;
      else inParamName = true;
    } else if (char === "-" || char === ".") {
      inParamName = false;
    } else if (char === "(" && inParamName) {
      i = getClosingParenthensePosition(prefix, i);
      inParamName = false;
    }
  }
  return count;
};

/** The request with the URL below the first `segments` segments. */
const sliceUrl = (request: ServerRequest, segments: number, ignoreDuplicateSlashes: boolean): ServerRequest => {
  const url = request.url;
  let index = 0;
  for (let left = segments; left > 0; left--) {
    if (ignoreDuplicateSlashes) while (url[index] === "/") index++;
    else if (url[index] === "/") index++;
    while (index < url.length && !"/?;#".includes(url[index] ?? "")) index++;
  }
  const rest = url.slice(index);
  return withUrl(request, rest.startsWith("/") ? rest : "/" + rest);
};

// the router takes any pattern, as Effect's does at run time; only its type asks for a leading `/`
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a pattern is given to the router as it is
const pathInput = (path: string): FindMyWay.PathInput => path as FindMyWay.PathInput;

interface Entry {
  readonly route: Route;
  readonly prefixSegments: number | undefined;
}

const pairsOf = (params: Readonly<Record<string, string | undefined>>): Pairs =>
  Object.entries(params).flatMap(([k, v]) => (v === undefined ? [] : [[k, v] as const]));

/** `Http.Router.router config routes`: one handler of the routes. */
export const flickRouter = (config: RouterConfig, routes: ReadonlyArray<Route>): Handler => {
  const given: { -readonly [K in keyof RouterConfig]?: RouterConfig[K] extends Option<infer A> ? A : never } = {};
  if (config.ignoreTrailingSlash._tag === "Some") given.ignoreTrailingSlash = config.ignoreTrailingSlash.value;
  if (config.ignoreDuplicateSlashes._tag === "Some") given.ignoreDuplicateSlashes = config.ignoreDuplicateSlashes.value;
  if (config.caseSensitive._tag === "Some") given.caseSensitive = config.caseSensitive.value;
  if (config.maxParamLength._tag === "Some") given.maxParamLength = config.maxParamLength.value;
  const ignoreDuplicateSlashes = given.ignoreDuplicateSlashes ?? true;
  const router = FindMyWay.make<Entry>(given);
  for (const route of routes) {
    const entry: Entry = { route, prefixSegments: route.prefix === undefined ? undefined : countSegments(route.prefix, ignoreDuplicateSlashes) };
    const paths = route.path.endsWith("/*") ? [pathInput(route.path), pathInput(route.path.slice(0, -2))] : [pathInput(route.path)];
    for (const p of paths) {
      if (route.method === "*") router.all(p, entry);
      else router.on(route.method, p, entry);
    }
  }
  return (request) => {
    let result = router.find(request.method, request.url);
    let paramLimit = result === undefined ? getMaxParamLengthExceeded(router) : undefined;
    if (result === undefined && request.method === "HEAD") {
      result = router.find("GET", request.url);
      paramLimit ??= getMaxParamLengthExceeded(router);
    }
    if (result === undefined) {
      const description = paramLimit === undefined ? undefined : `path parameter is longer than maxParamLength (${paramLimit})`;
      return raise(fail(httpServerError("RouteNotFound", request, description)));
    }
    const { route, prefixSegments } = result.handler;
    const seen = prefixSegments === undefined ? request : sliceUrl(request, prefixSegments, ignoreDuplicateSlashes);
    const params = pairsOf(result.params);
    const context = Object.freeze({ params: (_unit: undefined) => params });
    return new Under(extend(currentCtx(), ROUTE_CONTEXT, context), () => atMask(0, () => route.handler(seen)));
  };
};

export const flickParamsUnknown = (params: Pairs): unknown => Object.freeze(Object.fromEntries(params));

const isPairs = (value: unknown): value is Pairs =>
  Array.isArray(value) && value.every((pair) => Array.isArray(pair) && typeof pair[0] === "string" && typeof pair[1] === "string");

/** `then` of the parameters of the route that matched, from the `RouteContext` in effect. */
function* withParams(then: (params: Readonly<Record<string, string>>) => unknown): Run {
  const params = operation(ROUTE_CONTEXT, "params", 1);
  if (typeof params !== "function") throw new TypeError("RouteContext.params is no function");
  const pairs: unknown = yield Reflect.apply(params, undefined, [undefined]);
  return yield then(isPairs(pairs) ? Object.fromEntries(pairs) : {});
}

const record = (request: ServerRequest): Record<string, unknown> =>
  Object.fromEntries(flickToRecord(flickSearchParams(request)).map(([k, vs]) => [k, vs.length === 1 ? vs[0] : Object.freeze(vs)]));

export const flickSchemaParams = (codec: Witness, request: ServerRequest): Run =>
  withParams((params) => codecDecode(codec, Object.freeze({ ...record(request), ...params })));

const requestInput = (request: ServerRequest, params: Readonly<Record<string, string>>) => ({
  method: request.method,
  url: request.url,
  cookies: Object.freeze(Object.fromEntries(flickCookies(request).map(([k, v]) => [k, redactedReveal(v)]))),
  headers: headersToUnknown(request.headers),
  pathParams: Object.freeze({ ...params }),
  searchParams: flickToUnknown(flickSearchParams(request)),
});

export const flickSchemaNoBody = (codec: Witness, request: ServerRequest): Run =>
  withParams((params) => codecDecode(codec, Object.freeze(requestInput(request, params))));

export const flickSchemaJson = (codec: Witness, request: ServerRequest): Run =>
  withParams(function* (params): Run {
    const body: unknown = yield readJson(request);
    return yield codecDecode(codec, Object.freeze({ ...requestInput(request, params), body }));
  });
