// Effect's `HttpClientRequest` for the bindings of the standard library:
// methods as `Http.Method.Method`, options as records of `Option` fields.
import { Effect, Option, type Stream } from "effect";
import type * as Headers from "effect/http/Headers";
import type * as HttpBody from "effect/http/HttpBody";
import * as Request from "effect/http/HttpClientRequest";
import type * as UrlParams from "effect/http/UrlParams";

import type { Data } from "../../data.ts";
import { type AnySchema, codecEncode } from "../../schema/index.ts";
import { defined, enumeration, method, toBytes } from "./convert.ts";
import { flickStream, formData, mapError } from "./HttpBody.ts";

export * from "effect/http/HttpClientRequest";

type Pairs = ReadonlyArray<readonly [string, string]>;

/** `Http.ClientRequest.Options`. */
export interface FlickOptions {
  readonly urlParams: Option.Option<Pairs>;
  readonly hash: Option.Option<string>;
  readonly headers: Option.Option<Pairs>;
  readonly body: Option.Option<HttpBody.HttpBody>;
  readonly accept: Option.Option<string>;
  readonly acceptJson: boolean;
}

const options = (o: FlickOptions): Request.Options.NoUrl => defined(o);

export const flickMakeWith = (
  m: Data,
  url: string,
  params: UrlParams.UrlParams,
  hash: Option.Option<string>,
  headers: Headers.Headers,
  body: HttpBody.HttpBody,
) => Request.makeWith(method.js(m), url, params, hash, headers, body);

export const flickMake = (m: Data, url: string, o: FlickOptions) => Request.make(method.js(m))(url, options(o));

export const flickModify = (o: FlickOptions, self: Request.HttpClientRequest) => Request.modify(self, options(o));

export const flickSetMethod = (m: Data, self: Request.HttpClientRequest) => Request.setMethod(self, method.js(m));

const compression = enumeration<Request.CompressionAlgorithm>("Http.ClientRequest.CompressionAlgorithm", [
  ["Gzip", "gzip"],
  ["Deflate", "deflate"],
]);

export const flickCompressBody = (algorithm: Data, minSize: Option.Option<number>, self: Request.HttpClientRequest) =>
  Request.compressBody(self, compression.js(algorithm), { minSize: Option.getOrUndefined(minSize) });

export const flickBodyUint8Array = (
  bytes: ReadonlyArray<number>,
  contentType: Option.Option<string>,
  self: Request.HttpClientRequest,
) => Request.bodyUint8Array(self, toBytes(bytes), Option.getOrUndefined(contentType));

export const flickBodyJson = (value: unknown, self: Request.HttpClientRequest) =>
  mapError(Request.bodyJson(self, value));

export const flickSchemaBodyJson = (codec: AnySchema, value: unknown, self: Request.HttpClientRequest) =>
  Effect.flatMap(codecEncode(codec, value), (json) => flickBodyJson(json, self));

export const flickBodyStream = (
  chunks: Stream.Stream<ReadonlyArray<number>, unknown>,
  contentType: Option.Option<string>,
  contentLength: Option.Option<number>,
  self: Request.HttpClientRequest,
) => Request.setBody(self, flickStream(chunks, contentType, contentLength));

export const flickBodyFormData = (pairs: Pairs, self: Request.HttpClientRequest) =>
  Request.bodyFormData(self, formData(pairs));

export const flickMethod = (self: Request.HttpClientRequest): Data => method.flick(self.method);
export const flickUrl = (self: Request.HttpClientRequest): string => self.url;
export const flickUrlParams = (self: Request.HttpClientRequest): UrlParams.UrlParams => self.urlParams;
export const flickHash = (self: Request.HttpClientRequest): Option.Option<string> => self.hash;
export const flickHeaders = (self: Request.HttpClientRequest): Headers.Headers => self.headers;
export const flickBody = (self: Request.HttpClientRequest): HttpBody.HttpBody => self.body;
