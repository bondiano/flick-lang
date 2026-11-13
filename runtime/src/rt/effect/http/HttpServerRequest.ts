// `Http.ServerRequest` on Flick's own runtime: a request is the
// `ServerRequest` of `../../http/model.ts`, its body read as Effect's
// `HttpServerRequest` reads it. A request built from a client's request,
// and the client's request of one, are the adapter's until the client runs
// here.
import { type Option, none, some } from "../../../core/data.ts";
import { codecDecode } from "../../codec.ts";
import type { Run } from "../../fiber.ts";
import { type ServerRequest, methodToFlick } from "../../http/model.ts";
import { readBytes, readJson, readText, readUrlParams, withMaxBodySize } from "../../http/request.ts";
import { type Redacted, redactedReveal } from "../../redacted.ts";
import type { Witness } from "../../schema.ts";
import { flickParseHeader } from "./Cookies.ts";
import { type Headers, flickToUnknown as headersToUnknown } from "./Headers.ts";
import { type UrlParams, flickToUnknown, flickToRecord } from "./UrlParams.ts";

export const flickMethod = (self: ServerRequest) => methodToFlick(self.method);
export const flickUrl = (self: ServerRequest): string => self.url;
export const flickOriginalUrl = (self: ServerRequest): string => self.originalUrl;
export const flickHeaders = (self: ServerRequest): Headers => self.headers;
export const flickRemoteAddress = (self: ServerRequest): Option<string> => self.remoteAddress;

/** The cookies of the `Cookie` header, the first of a name kept. */
export const flickCookies = (self: ServerRequest): ReadonlyArray<readonly [string, Redacted<string>]> =>
  flickParseHeader(self.headers["cookie"] ?? "");

export const cookieRecord = (self: ServerRequest): Readonly<Record<string, string>> =>
  Object.freeze(Object.fromEntries(flickCookies(self).map(([k, v]) => [k, redactedReveal(v)])));

/** The parameters of the query of the request's URL. */
export const flickSearchParams = (self: ServerRequest): UrlParams => [...new URL(self.url, "http://localhost").searchParams];

/** Each query key with its values, following the adapter's object enumeration order. */
export const flickSearchParamsFromUrl = (url: URL): ReadonlyArray<readonly [string, ReadonlyArray<string>]> =>
  Object.entries(Object.fromEntries(flickToRecord([...url.searchParams])));

export const flickText = (self: ServerRequest): Run => readText(self);
export const flickJson = (self: ServerRequest): Run => readJson(self);
export const flickUrlParamsBody = (self: ServerRequest): Run => readUrlParams(self);

export function* flickBytes(self: ServerRequest): Run {
  const bytes: unknown = yield readBytes(self);
  return bytes instanceof Uint8Array ? Array.from(bytes) : [];
}

export const flickWithMaxBodySize = withMaxBodySize;

/** The full URL of the request, its host from the `Host` header. */
export const toURL = (self: ServerRequest): Option<URL> => {
  const host = self.headers["host"] ?? "localhost";
  const protocol = /^https:/i.test(self.originalUrl) ? "https" : "http";
  // a leading `//` would replace the host
  const url = self.url.startsWith("/") ? `.${self.url}` : self.url;
  return URL.canParse(url, `${protocol}://${host}`) ? some(new URL(url, `${protocol}://${host}`)) : none;
};

export const flickSchemaCookies = (codec: Witness, self: ServerRequest): Run => codecDecode(codec, cookieRecord(self));

export const flickSchemaHeaders = (codec: Witness, self: ServerRequest): Run => codecDecode(codec, headersToUnknown(self.headers));

export const flickSchemaSearchParams = (codec: Witness, self: ServerRequest): Run =>
  codecDecode(codec, flickToUnknown(flickSearchParams(self)));

export function* flickSchemaBodyJson(codec: Witness, self: ServerRequest): Run {
  return yield codecDecode(codec, yield readJson(self));
}

export const flickSchemaBodyUrlParams = (codec: Witness, self: ServerRequest): Run =>
  readUrlParams(self, (params) => codecDecode(codec, flickToUnknown(params)));
