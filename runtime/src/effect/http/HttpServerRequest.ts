// Effect's `HttpServerRequest` for the bindings of the standard library:
// reading the body fails with the Flick `Http.ServerError.HttpServerError`.
import { Effect, type Option, Redacted } from "effect";
import type * as ByteSize from "effect/ByteSize";
import * as Request from "effect/http/HttpServerRequest";
import * as UrlParams from "effect/http/UrlParams";

import type { Data } from "../../data.ts";
import { type AnySchema, codecDecode } from "../../schema/index.ts";
import { fromBytes, method } from "./convert.ts";
import { flickToUnknown as headersToUnknown } from "./Headers.ts";
import { mapError } from "./HttpServerError.ts";
import { flickToUnknown } from "./UrlParams.ts";

export * from "effect/http/HttpServerRequest";

type R = Request.HttpServerRequest;

export const flickMethod = (self: R): Data => method.flick(self.method);
export const flickUrl = (self: R): string => self.url;
export const flickOriginalUrl = (self: R): string => self.originalUrl;
export const flickHeaders = (self: R) => self.headers;
export const flickRemoteAddress = (self: R): Option.Option<string> => self.remoteAddress;
export const flickCookies = (self: R) => Object.entries(self.cookies).map(([k, v]) => [k, Redacted.make(v)] as const);

const query = (url: string): URLSearchParams => new URL(url, "http://localhost").searchParams;

/** `Http.ServerRequest.search_params`. */
export const flickSearchParams = (self: R): UrlParams.UrlParams => UrlParams.fromInput(query(self.url));

/** `Http.ServerRequest.search_params_from_url`. */
export const flickSearchParamsFromUrl = (url: URL): ReadonlyArray<readonly [string, ReadonlyArray<string>]> =>
  Object.entries(Request.searchParamsFromURL(url)).map(([k, v]) => [k, typeof v === "string" ? [v] : v] as const);

export const flickText = (self: R) => mapError(self.text);
export const flickJson = (self: R) => mapError(self.json);
export const flickUrlParamsBody = (self: R) => mapError(self.urlParamsBody);
export const flickBytes = (self: R) => mapError(Effect.map(self.arrayBuffer, fromBytes));

export const flickWithMaxBodySize = <A, E, Req>(size: ByteSize.ByteSize, effect: Effect.Effect<A, E, Req>) =>
  Effect.provideService(effect, Request.MaxBodySize, size);

export const flickSchemaCookies = (codec: AnySchema, self: R) => codecDecode(codec, Object.freeze({ ...self.cookies }));

export const flickSchemaHeaders = (codec: AnySchema, self: R) => codecDecode(codec, headersToUnknown(self.headers));

export const flickSchemaSearchParams = (codec: AnySchema, self: R) =>
  codecDecode(codec, flickToUnknown(flickSearchParams(self)));

export const flickSchemaBodyJson = (codec: AnySchema, self: R) =>
  Effect.flatMap(flickJson(self), (body) => codecDecode(codec, body));

export const flickSchemaBodyUrlParams = (codec: AnySchema, self: R) =>
  Effect.flatMap(flickUrlParamsBody(self), (params) => codecDecode(codec, flickToUnknown(params)));
