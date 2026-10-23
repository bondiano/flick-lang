// `HttpApi` values: endpoints, groups and APIs as frozen data. Labels are
// the names the compiler passes for `Record.Label`; the codecs are Flick's
// (Effect Schema codecs of the JSON wire form).
import type { AnySchema } from "../schema/schemas.ts";

export type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "QUERY";

/** `HttpApi.Security`: how a client proves who it is. */
export type Security =
  | { readonly kind: "bearer"; readonly name: string }
  | { readonly kind: "basic"; readonly name: string }
  | { readonly kind: "apiKey"; readonly name: string; readonly key: string; readonly in: "header" | "query" | "cookie" };

/** `HttpApi.ErrorCase`: a typed error with its status and codec. */
export interface ErrorCase {
  readonly status: number;
  readonly codec: AnySchema;
}

/** `HttpApi.Endpoint l f`. */
export interface Endpoint {
  readonly name: string;
  readonly method: Method;
  readonly path: string;
  readonly params: AnySchema | null;
  readonly query: AnySchema | null;
  readonly headers: AnySchema | null;
  readonly payload: AnySchema | null;
  /** How the body is written: JSON, plain text or a URL-encoded form. */
  readonly payloadEncoding: "json" | "text" | "form";
  readonly status: number;
  readonly successEncoding: "json" | "text";
  readonly success: AnySchema | null;
  readonly errors: ReadonlyArray<ErrorCase>;
  readonly description: string;
  readonly security: ReadonlyArray<Security>;
  /** Run in order before the handler, on the decoded request; a failure answers instead. */
  readonly middleware: ReadonlyArray<(request: unknown) => unknown>;
  readonly deprecated: boolean;
  /** Left out of the OpenAPI document. */
  readonly hidden: boolean;
}

/** `HttpApi.Group n hs`. */
export interface Group {
  readonly name: string;
  readonly endpoints: ReadonlyArray<Endpoint>;
}

/** What the OpenAPI document says of the API besides its endpoints. */
export interface Info {
  readonly version: string;
  readonly description: string;
  readonly servers: ReadonlyArray<string>;
}

/** `HttpApi.Api gs`. */
export interface Api {
  readonly name: string;
  readonly info: Info;
  readonly groups: ReadonlyArray<Group>;
}

/** The ADT value of `HttpApi.Method`, by its constructor name. */
const METHODS: Readonly<Record<string, Method>> = {
  Get: "GET",
  Post: "POST",
  Put: "PUT",
  Patch: "PATCH",
  Delete: "DELETE",
  Head: "HEAD",
  Options: "OPTIONS",
  Query: "QUERY",
};

const methodOf = (value: { readonly _tag: string }): Method => {
  const name = value._tag.slice(0, value._tag.indexOf("#") === -1 ? undefined : value._tag.indexOf("#"));
  const method = METHODS[name];
  if (method === undefined) throw new TypeError(`not an HTTP method: ${value._tag}`);
  return method;
};

export const httpApiEndpoint = (method: { readonly _tag: string }, path: string, name: string): Endpoint =>
  Object.freeze({
    name,
    method: methodOf(method),
    path,
    params: null,
    query: null,
    headers: null,
    payload: null,
    payloadEncoding: "json",
    status: 204,
    successEncoding: "json",
    success: null,
    errors: [],
    description: "",
    security: [],
    middleware: [],
    deprecated: false,
    hidden: false,
  });

const update = (endpoint: Endpoint, change: Partial<Endpoint>): Endpoint => Object.freeze({ ...endpoint, ...change });

export const httpApiWithPath = (codec: AnySchema, e: Endpoint): Endpoint => update(e, { params: codec });
export const httpApiWithQuery = (codec: AnySchema, e: Endpoint): Endpoint => update(e, { query: codec });
export const httpApiWithHeaders = (codec: AnySchema, e: Endpoint): Endpoint => update(e, { headers: codec });
export const httpApiWithPayload = (codec: AnySchema, e: Endpoint): Endpoint => update(e, { payload: codec });
export const httpApiWithTextPayload = (text: AnySchema, e: Endpoint): Endpoint =>
  update(e, { payload: text, payloadEncoding: "text" });
export const httpApiWithFormPayload = (codec: AnySchema, e: Endpoint): Endpoint =>
  update(e, { payload: codec, payloadEncoding: "form" });
export const httpApiWithTextSuccess = (status: number, text: AnySchema, e: Endpoint): Endpoint =>
  update(e, { status, success: text, successEncoding: "text" });
export const httpApiWithSuccess = (status: number, codec: AnySchema, e: Endpoint): Endpoint =>
  update(e, { status, success: codec });
export const httpApiWithErrors = (errors: ReadonlyArray<ErrorCase>, e: Endpoint): Endpoint =>
  update(e, { errors: [...e.errors, ...errors] });
export const httpApiDescribe = (description: string, e: Endpoint): Endpoint => update(e, { description });
export const httpApiErrorCase = (status: number, codec: AnySchema): ErrorCase => Object.freeze({ status, codec });

export const httpApiWithSecurity = (security: Security, e: Endpoint): Endpoint =>
  update(e, { security: [...e.security, security] });
export const httpApiWithMiddleware = (hook: (request: unknown) => unknown, e: Endpoint): Endpoint =>
  update(e, { middleware: [...e.middleware, hook] });
export const httpApiDeprecate = (e: Endpoint): Endpoint => update(e, { deprecated: true });
export const httpApiHide = (e: Endpoint): Endpoint => update(e, { hidden: true });

export const httpApiBearer = (name: string): Security => Object.freeze({ kind: "bearer", name });
export const httpApiBasic = (name: string): Security => Object.freeze({ kind: "basic", name });

/** `HttpApi.api_key name { key; location }`; the location is `Header`, `Query` or `Cookie`. */
export const httpApiKey = (name: string, options: { readonly key: string; readonly location: { readonly _tag: string } }): Security => {
  const where = options.location._tag.split("#")[0];
  const place = where === "Query" ? "query" : where === "Cookie" ? "cookie" : "header";
  return Object.freeze({ kind: "apiKey", name, key: options.key, in: place });
};

export const httpApiGroup = (name: string): Group => Object.freeze({ name, endpoints: [] });

export const httpApiAdd = (_lacks: unknown, _union: unknown, endpoint: Endpoint, group: Group): Group =>
  Object.freeze({ ...group, endpoints: [...group.endpoints, endpoint] });

export const httpApiMake = (name: string): Api =>
  Object.freeze({ name, info: { version: "0.1.0", description: "", servers: [] }, groups: [] });

export const httpApiInfo = (info: Info, api: Api): Api => Object.freeze({ ...api, info });

export const httpApiAddGroup = (_lacks: unknown, _union: unknown, group: Group, api: Api): Api =>
  Object.freeze({ ...api, groups: [...api.groups, group] });
