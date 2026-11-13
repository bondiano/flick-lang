// `Http.ServerResponse` on Flick's own runtime: a response is the immutable
// `ServerResponse` of `../../http/model.ts`, made and changed as Effect's
// `HttpServerResponse` is, so its headers come out the same. Streamed
// bodies and the conversions to and from the client's responses are the
// adapter's until Flick's streams and client run here.
import { type Option, optionalArg } from "../../../core/data.ts";
import { codecEncode } from "../../codec.ts";
import type { Run } from "../../fiber.ts";
import {
  type Body,
  type ResponseOptions,
  type ServerResponse,
  bodyFormData,
  bodyBytes,
  bodyResponse,
  bodyText,
  causeResponse,
  emptyResponse,
  makeResponse,
  omitsBody,
  redirectResponse,
  updateHeaders,
} from "../../http/model.ts";
import type { Failure } from "../../outcome.ts";
import type { Witness } from "../../schema.ts";
import { type Cookies, flickExpireCookie as expire, flickSet, flickSetAll, merge, remove as removeNamed } from "./Cookies.ts";
import { formData, jsonText } from "./HttpBody.ts";
import { type Headers, fromInput, remove, set, setAll } from "./Headers.ts";
import { type UrlParams, toString } from "./UrlParams.ts";

type Pairs = ReadonlyArray<readonly [string, string]>;

/** `Http.ServerResponse.Options`. */
export interface FlickOptions {
  readonly status: Option<number>;
  readonly statusText: Option<string>;
  readonly headers: Option<Pairs>;
  readonly cookies: Option<Cookies>;
  readonly contentType: Option<string>;
}

/** The options of the Flick record, a field of `None` left out. */
export const options = (o: FlickOptions): ResponseOptions => ({
  status: optionalArg(o.status),
  statusText: optionalArg(o.statusText),
  headers: optionalArg(o.headers),
  cookies: optionalArg(o.cookies),
  contentType: optionalArg(o.contentType),
});

export const flickEmpty = (o: FlickOptions): ServerResponse => emptyResponse(options(o));
export const flickRedirect = (location: string, o: FlickOptions): ServerResponse => redirectResponse(location, options(o));
export const flickUint8Array = (bytes: ReadonlyArray<number>, o: FlickOptions): ServerResponse =>
  bodyResponse(options(o), (type) => bodyBytes(Uint8Array.from(bytes), type));
export const flickText = (body: string, o: FlickOptions): ServerResponse => bodyResponse(options(o), (type) => bodyText(body, type));
export const html = (body: string): ServerResponse => bodyResponse({ contentType: "text/html" }, (type) => bodyText(body, type));
export const flickJson = (body: unknown, o: FlickOptions): ServerResponse => {
  const text = jsonText(body);
  return bodyResponse(options(o), (type) => bodyText(text, type ?? "application/json"));
};
export function* flickSchemaJson(codec: Witness, body: unknown, o: FlickOptions): Run {
  return flickJson(yield codecEncode(codec, body), o);
}
export const flickUrlParams = (params: UrlParams, o: FlickOptions): ServerResponse =>
  bodyResponse(options(o), (type) => bodyText(toString(params), type ?? "application/x-www-form-urlencoded"));

/** A form body: Effect takes neither its content type nor its length from the options. */
export const flickFormData = (pairs: Pairs, o: FlickOptions): ServerResponse => {
  const given = options(o);
  return makeResponse({
    status: given.status ?? 200,
    statusText: given.statusText,
    headers: given.headers === undefined ? undefined : fromInput(given.headers),
    cookies: given.cookies,
    body: bodyFormData(formData(pairs)),
  });
};

export const flickStatus = (self: ServerResponse): number => self.status;
export const flickStatusText = (self: ServerResponse): Option<string> =>
  self.statusText === undefined ? { _tag: "None" } : { _tag: "Some", value: self.statusText };
export const flickHeaders = (self: ServerResponse): Headers => self.headers;
export const flickCookies = (self: ServerResponse): Cookies => self.cookies;
export const flickBody = (self: ServerResponse): Body => self.body;

const withParts = (self: ServerResponse, change: Partial<Pick<ServerResponse, "status" | "statusText" | "headers" | "cookies" | "body">>): ServerResponse =>
  makeResponse({ status: self.status, statusText: self.statusText, headers: self.headers, cookies: self.cookies, body: self.body, ...change });

/** A response with headers that win over those of its body. */
const withHeaders = (self: ServerResponse, headers: Headers): ServerResponse =>
  makeResponse({ status: self.status, statusText: self.statusText, cookies: self.cookies, body: self.body }, headers);

export const setHeader = (name: string, text: string) => (self: ServerResponse): ServerResponse => withHeaders(self, set(name, text)(self.headers));
// as with Effect, the body puts back a content header removed from a response that has one
export const removeHeader = (name: string) => (self: ServerResponse): ServerResponse => withParts(self, { headers: remove(name)(self.headers) });
export const setHeaders = (input: Pairs) => (self: ServerResponse): ServerResponse => withHeaders(self, setAll(input)(self.headers));
export const removeCookie = (name: string) => (self: ServerResponse): ServerResponse => withParts(self, { cookies: removeNamed(name)(self.cookies) });
export const replaceCookies = (cookies: Cookies) => (self: ServerResponse): ServerResponse => withParts(self, { cookies });
export const updateCookies = (f: (cookies: Cookies) => Cookies) => (self: ServerResponse): ServerResponse => withParts(self, { cookies: f(self.cookies) });
export const mergeCookies = (cookies: Cookies) => (self: ServerResponse): ServerResponse => withParts(self, { cookies: merge(cookies)(self.cookies) });

export const flickSetCookie = (name: string, raw: Parameters<typeof flickSet>[1], a: Parameters<typeof flickSet>[2], self: ServerResponse): ServerResponse =>
  withParts(self, { cookies: flickSet(name, raw, a, self.cookies) });
export const flickExpireCookie = (name: string, a: Parameters<typeof expire>[1], self: ServerResponse): ServerResponse =>
  withParts(self, { cookies: expire(name, a, self.cookies) });
export const flickSetCookies = (entries: Parameters<typeof flickSetAll>[0], self: ServerResponse): ServerResponse =>
  withParts(self, { cookies: flickSetAll(entries, self.cookies) });

/** A new body, the content headers following it. */
export const setBody = (body: Body) => (self: ServerResponse): ServerResponse =>
  makeResponse({ status: self.status, statusText: self.statusText, cookies: self.cookies, body }, updateHeaders(self.headers, body));

/** The status, and the status text if given, or the one it had. */
export const setStatus = (status: number, statusText?: string) => (self: ServerResponse): ServerResponse =>
  withParts(self, { status, statusText: statusText ?? self.statusText });

export const flickOmitsBody = (withoutBody: boolean, self: ServerResponse): boolean => omitsBody(self, withoutBody);

/** `cause_response failure`: the response a server answers the failure with. */
export const flickCauseResponse = (failure: Failure): ServerResponse => causeResponse(failure.cause);
