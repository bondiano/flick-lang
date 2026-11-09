// Effect's `HttpClientResponse` for the bindings of the standard library:
// reading the body fails with the Flick `Http.ClientError.HttpClientError`.
import { Effect, Option, Stream } from "effect";
import * as HttpClientError from "effect/http/HttpClientError";
import * as Response from "effect/http/HttpClientResponse";

import { type AnySchema, codecDecode } from "../../schema/index.ts";
import { fromBytes } from "./convert.ts";
import { flickToUnknown as headersToUnknown } from "./Headers.ts";
import { mapError, toFlick } from "./HttpClientError.ts";
import { flickToUnknown } from "./UrlParams.ts";

export * from "effect/http/HttpClientResponse";

type R = Response.HttpClientResponse;

export const flickStatus = (self: R) => self.status;
export const flickHeaders = (self: R) => self.headers;
export const flickCookies = (self: R) => self.cookies;
export const flickUrl = (self: R) => self.url;
export const flickRequest = (self: R) => self.request;
export const flickText = (self: R) => mapError(self.text);
export const flickJson = (self: R) => mapError(self.json);
export const flickUrlParamsBody = (self: R) => mapError(self.urlParamsBody);
export const flickBytes = (self: R) => mapError(Effect.map(self.arrayBuffer, fromBytes));

export const flickStream = (self: R) =>
  Stream.map(
    Stream.mapError(self.stream, (e) => (HttpClientError.isHttpClientError(e) ? toFlick(e) : e)),
    fromBytes,
  );

/** The text fields of a form body. */
export const flickFormData = (self: R) =>
  mapError(
    Effect.map(self.formData, (data) =>
      [...data.entries()].flatMap(([k, v]) => (typeof v === "string" ? [[k, v] as const] : [])),
    ),
  );

export const flickFilterStatus = (f: (status: number) => boolean, self: R) =>
  mapError(Response.filterStatus(self, f));

export const flickFilterStatusOk = (self: R) => mapError(Response.filterStatusOk(self));

/** `{ status, headers, body? }` of the response, inert for a codec. */
const schemaInput = (self: R, body: Option.Option<unknown>): unknown =>
  Object.freeze({
    status: self.status,
    headers: Object.freeze({ ...self.headers }),
    ...(Option.isSome(body) ? { body: body.value } : {}),
  });

export const flickSchemaJson = (codec: AnySchema, self: R) =>
  Effect.flatMap(flickJson(self), (body) => codecDecode(codec, schemaInput(self, Option.some(body))));

export const flickSchemaNoBody = (codec: AnySchema, self: R) => codecDecode(codec, schemaInput(self, Option.none()));

export const flickSchemaBodyJson = (codec: AnySchema, self: R) =>
  Effect.flatMap(flickJson(self), (body) => codecDecode(codec, body));

export const flickSchemaBodyUrlParams = (codec: AnySchema, self: R) =>
  Effect.flatMap(flickUrlParamsBody(self), (params) => codecDecode(codec, flickToUnknown(params)));

export const flickSchemaHeaders = (codec: AnySchema, self: R) => codecDecode(codec, headersToUnknown(self.headers));
