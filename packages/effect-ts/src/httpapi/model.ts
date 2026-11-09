// `HttpApi` values: endpoints, groups and APIs as frozen data. Labels are
// the names the compiler passes for `Record.Label`; the codecs are Flick's
// (Effect Schema codecs of the JSON wire form).
import { type Context, Schema } from "effect";
import type { AnySchema } from "../schema/schemas.ts";

export type Method = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" | "QUERY";

/** `HttpApi.Security`: how a client proves who it is. */
export type Security =
  | { readonly kind: "bearer"; readonly name: string }
  | { readonly kind: "basic"; readonly name: string }
  | { readonly kind: "apiKey"; readonly name: string; readonly key: string; readonly in: "header" | "query" | "cookie" };

/** `HttpApi.with_auth`: the service `tag` that `check` makes from each request, for the handler. */
export interface Auth {
  readonly security: Security;
  readonly tag: Context.Key<unknown, unknown>;
  readonly check: (request: unknown) => unknown;
  /** Anonymous requests are served too: the document's security is optional. */
  readonly optional: boolean;
}

/** One of `HttpApi.ErrorCases`: a typed error with its status and codec. */
export interface ErrorCase {
  readonly status: number;
  readonly codec: AnySchema;
}

export type PayloadEncoding = "json" | "text" | "form" | "multipart" | "bytes" | "parts";

export type SuccessEncoding = "json" | "text" | "bytes" | "stream" | "sse" | "events";

/** `(value, extra)` answered by the handler: headers of the codec, or the value of the cookie `key`. */
export type SuccessExtra = { readonly kind: "headers"; readonly codec: AnySchema } | { readonly kind: "cookie"; readonly key: string };

/** A value of the type the checker gave it by the endpoint: a handler's value or a call's payload. */
// oxlint-disable-next-line typescript/no-unsafe-type-assertion, typescript/no-unnecessary-type-parameters -- the checker typed the value by the endpoint's signature
export const typed = <A>(value: unknown): A => value as A;

/** Whether the endpoint reads a body: one of a codec, or bytes or parts. */
export const hasPayload = (e: Endpoint): boolean => e.payload !== null || e.payloadEncoding === "bytes" || e.payloadEncoding === "parts";

/** Whether the success has a body: one of a codec, or bytes, a stream or events. */
export const hasSuccessBody = (e: Endpoint): boolean =>
  e.success !== null || e.successEncoding === "bytes" || e.successEncoding === "stream" || e.successEncoding === "events";

/** `HttpApi.Endpoint l f`. */
export interface Endpoint {
  readonly name: string;
  readonly method: Method;
  readonly path: string;
  readonly params: AnySchema | null;
  readonly query: AnySchema | null;
  readonly headers: AnySchema | null;
  readonly payload: AnySchema | null;
  /**
   * How the body is written: JSON, plain text, a URL-encoded or multipart
   * form, bytes, or the parts of a form as a stream (these two without a codec).
   */
  readonly payloadEncoding: PayloadEncoding;
  readonly status: number;
  /**
   * How the success is written: JSON, plain text, bytes, a stream of bytes,
   * Server-Sent Events of JSON data (with the codec of the data) or events.
   */
  readonly successEncoding: SuccessEncoding;
  readonly success: AnySchema | null;
  /** What the handler answers besides the value, innermost first: response headers, a cookie. */
  readonly successExtras: ReadonlyArray<SuccessExtra>;
  readonly errors: ReadonlyArray<ErrorCase>;
  readonly description: string;
  readonly security: ReadonlyArray<Security>;
  /** Run in order before the handler, on the decoded request; a failure answers instead. */
  readonly middleware: ReadonlyArray<(request: unknown) => unknown>;
  /** Makes a service for the handler after the middleware. */
  readonly auth: Auth | null;
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
    successExtras: [],
    errors: [],
    description: "",
    security: [],
    middleware: [],
    auth: null,
    deprecated: false,
    hidden: false,
  });

const update = (endpoint: Endpoint, change: Partial<Endpoint>): Endpoint => Object.freeze({ ...endpoint, ...change });

export const httpApiWithPath = (codec: AnySchema, e: Endpoint): Endpoint => update(e, { params: codec });
export const httpApiWithQuery = (codec: AnySchema, e: Endpoint): Endpoint => update(e, { query: codec });
export const httpApiWithHeaders = (codec: AnySchema, e: Endpoint): Endpoint => update(e, { headers: codec });
export const httpApiWithPayload = (codec: AnySchema, e: Endpoint): Endpoint =>
  update(e, { payload: codec, payloadEncoding: "json" });
export const httpApiWithTextPayload = (text: AnySchema, e: Endpoint): Endpoint =>
  update(e, { payload: text, payloadEncoding: "text" });
export const httpApiWithFormPayload = (codec: AnySchema, e: Endpoint): Endpoint =>
  update(e, { payload: codec, payloadEncoding: "form" });
export const httpApiWithTextSuccess = (status: number, text: AnySchema, e: Endpoint): Endpoint =>
  update(e, { status, success: text, successEncoding: "text" });
export const httpApiWithSuccess = (status: number, codec: AnySchema, e: Endpoint): Endpoint =>
  update(e, { status, success: codec, successEncoding: "json" });
export const httpApiWithBytesPayload = (e: Endpoint): Endpoint => update(e, { payload: null, payloadEncoding: "bytes" });
export const httpApiWithMultipartPayload = (codec: AnySchema, e: Endpoint): Endpoint =>
  update(e, { payload: codec, payloadEncoding: "multipart" });
export const httpApiWithMultipartStreamPayload = (e: Endpoint): Endpoint => update(e, { payload: null, payloadEncoding: "parts" });
export const httpApiWithBytesSuccess = (status: number, e: Endpoint): Endpoint =>
  update(e, { status, success: null, successEncoding: "bytes" });
export const httpApiWithStreamSuccess = (status: number, e: Endpoint): Endpoint =>
  update(e, { status, success: null, successEncoding: "stream" });
export const httpApiWithSseSuccess = (status: number, codec: AnySchema, e: Endpoint): Endpoint =>
  update(e, { status, success: codec, successEncoding: "sse" });
export const httpApiWithSseEventsSuccess = (status: number, e: Endpoint): Endpoint =>
  update(e, { status, success: null, successEncoding: "events" });
export const httpApiWithSuccessHeaders = (codec: AnySchema, e: Endpoint): Endpoint =>
  update(e, { successExtras: [...e.successExtras, { kind: "headers", codec }] });

/** `HttpApi.security_set_cookie`: the security must be an API key in a cookie. */
export const httpApiSecuritySetCookie = (security: Security, e: Endpoint): Endpoint => {
  if (security.kind !== "apiKey" || security.in !== "cookie") {
    throw new TypeError(`security_set_cookie needs an API key in a cookie: ${security.name}`);
  }
  return update(e, { successExtras: [...e.successExtras, { kind: "cookie", key: security.key }] });
};
/** A value no error type has: a codec that takes it takes anything. */
const PROBE = Object.freeze({ _tag: "flick/opaque-probe" });

/**
 * `HttpApi.with_errors cases endpoint`. Of several cases each must tell its
 * errors from the others', or a response would take the first case's status.
 */
export const httpApiWithErrors = (errors: ReadonlyArray<ErrorCase>, e: Endpoint): Endpoint => {
  const all = [...e.errors, ...errors];
  if (all.length > 1 && all.some((c) => Schema.is(c.codec)(PROBE))) {
    throw new TypeError(`the errors of ${e.name} cannot be told apart: give the codec of each a type with Codec.transform_as`);
  }
  return update(e, { errors: all });
};
export const httpApiDescribe = (description: string, e: Endpoint): Endpoint => update(e, { description });
/** `HttpApi.errors`: no case. */
export const httpApiErrors: ReadonlyArray<ErrorCase> = Object.freeze([]);
/** `HttpApi.error_case status codec cases`. */
export const httpApiErrorCase = (status: number, codec: AnySchema, cases: ReadonlyArray<ErrorCase>): ReadonlyArray<ErrorCase> =>
  Object.freeze([...cases, Object.freeze({ status, codec })]);

export const httpApiWithSecurity = (security: Security, e: Endpoint): Endpoint =>
  update(e, { security: [...e.security, security] });
export const httpApiWithMiddleware = (hook: (request: unknown) => unknown, e: Endpoint): Endpoint =>
  update(e, { middleware: [...e.middleware, hook] });
/** `HttpApi.with_auth security tag check endpoint`. */
export const httpApiWithAuth = (
  security: Security,
  tag: Context.Key<unknown, unknown>,
  check: (request: unknown) => unknown,
  e: Endpoint,
): Endpoint => update(e, { auth: Object.freeze({ security, tag, check, optional: false }) });
/** `HttpApi.with_optional_auth security tag check endpoint`. */
export const httpApiWithOptionalAuth = (
  security: Security,
  tag: Context.Key<unknown, unknown>,
  check: (request: unknown) => unknown,
  e: Endpoint,
): Endpoint => update(e, { auth: Object.freeze({ security, tag, check, optional: true }) });
export const httpApiDeprecate = (e: Endpoint): Endpoint => update(e, { deprecated: true });
export const httpApiHide = (e: Endpoint): Endpoint => update(e, { hidden: true });

export const httpApiBearer = (name: string): Security => Object.freeze({ kind: "bearer", name });
export const httpApiBasic = (name: string): Security => Object.freeze({ kind: "basic", name });

/** `HttpApi.api_key name { key; location }`; the location is `InHeader`, `InQuery` or `InCookie`. */
export const httpApiKey = (name: string, options: { readonly key: string; readonly location: { readonly _tag: string } }): Security => {
  const where = options.location._tag.split("#")[0];
  const place = where === "InQuery" ? "query" : where === "InCookie" ? "cookie" : "header";
  return Object.freeze({ kind: "apiKey", name, key: options.key, in: place });
};

export const httpApiGroup = (name: string): Group => Object.freeze({ name, endpoints: [] });

export const httpApiAdd = (_lacks: unknown, _union: unknown, endpoint: Endpoint, group: Group): Group =>
  Object.freeze({ ...group, endpoints: [...group.endpoints, endpoint] });

/** `HttpApi.authenticated tag group`: every endpoint makes the service `tag` for its handler. */
export const httpApiAuthenticated = (tag: Context.Key<unknown, unknown>, group: Group): Group => {
  const missing = group.endpoints.find((e) => e.auth?.tag !== tag);
  if (missing !== undefined) throw new TypeError(`the endpoint ${missing.name} has no \`with_auth\` making ${tag.key}`);
  return group;
};

export const httpApiMake = (name: string): Api =>
  Object.freeze({ name, info: { version: "0.1.0", description: "", servers: [] }, groups: [] });

export const httpApiInfo = (info: Info, api: Api): Api => Object.freeze({ ...api, info });

export const httpApiAddGroup = (_lacks: unknown, _union: unknown, group: Group, api: Api): Api =>
  Object.freeze({ ...api, groups: [...api.groups, group] });

/** `HttpApi.Mount.under prefix api`: every endpoint's path after `prefix`. */
export const httpApiUnder = (prefix: string, api: Api): Api =>
  Object.freeze({
    ...api,
    groups: api.groups.map((group) =>
      Object.freeze({ ...group, endpoints: group.endpoints.map((e) => Object.freeze({ ...e, path: joinPath(prefix, e.path) })) }),
    ),
  });

/** `/api` and `/users` as `/api/users`, whatever slashes either has. */
const joinPath = (prefix: string, path: string): string => `/${[prefix, path].map((p) => p.replace(/^\/+|\/+$/g, "")).filter((p) => p !== "").join("/")}`;
