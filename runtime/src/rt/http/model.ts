// The values of an HTTP server on Flick's own runtime, as Effect's
// `HttpServerRequest`, `HttpServerResponse`, `HttpBody` and
// `HttpServerError` have them, so that a server answers alike on both
// runtimes. The bindings of `Http.Server*`, `Http.Body`, `Http.Router` and
// `Http.Platform`, and the servers built on them, share these:
//
//   ServerRequest  a frozen view of a request: method, URL, headers, the
//                  client's address and the source its body is read from;
//                  a router's view of it changes only the URL.
//   ServerResponse an immutable `ServerResponse` of status, status text,
//                  headers, cookies and body.
//   Body           `Empty`, `Uint8Array` (bytes, with the text they encode),
//                  `FormData`, or `Raw` (a Node readable a file is sent from).
//
// A handler is a Flick function of a request; what it returns is a
// response, or a call that gives one. Its failures become responses by
// `causeResponse`.
import { type Option, none, some } from "../../core/data.ts";
import { tag } from "../../tags.ts";
import { type Cause, isFail } from "../cause.ts";
import { type Cookies, empty as cookiesEmpty } from "../effect/http/Cookies.ts";
import { type Headers, empty as headersEmpty, fromInput, merge } from "../effect/http/Headers.ts";
import { flickRedactUrl, redactedNames } from "../effect/http/UrlParams.ts";

/** Reads the bytes of a request's body once, at most `maxBytes` of them. */
export interface RequestSource {
  readonly read: (maxBytes: number | undefined, signal: AbortSignal) => Promise<Uint8Array>;
}

/** `Http.ServerRequest.ServerRequest`. */
export interface ServerRequest {
  /** Upper case, as the client sent it. */
  readonly method: string;
  /** The path and query, below the prefix a router took. */
  readonly url: string;
  readonly originalUrl: string;
  readonly headers: Headers;
  readonly remoteAddress: Option<string>;
  readonly source: RequestSource;
}

/** The request with the URL a router leaves for its route. */
export const withUrl = (request: ServerRequest, url: string): ServerRequest => Object.freeze({ ...request, url });

/** `Http.Body.Body`. */
export type Body =
  | Readonly<{ _tag: "Empty" }>
  | Readonly<{ _tag: "Uint8Array"; body: Uint8Array; text: string | undefined; contentType: string; contentLength: number }>
  | Readonly<{ _tag: "FormData"; formData: FormData }>
  | Readonly<{ _tag: "Raw"; body: unknown; contentType: string | undefined; contentLength: number | undefined }>;

export const bodyEmpty: Body = /*#__PURE__*/ Object.freeze({ _tag: "Empty" });

export const bodyBytes = (bytes: Uint8Array, contentType?: string): Body =>
  Object.freeze({ _tag: "Uint8Array", body: bytes, text: undefined, contentType: contentType ?? "application/octet-stream", contentLength: bytes.length });

const encoder = /*#__PURE__*/ new TextEncoder();

export const bodyText = (text: string, contentType?: string): Body => {
  const bytes = encoder.encode(text);
  return Object.freeze({ _tag: "Uint8Array", body: bytes, text, contentType: contentType ?? "text/plain", contentLength: bytes.length });
};

export const bodyFormData = (formData: FormData): Body => Object.freeze({ _tag: "FormData", formData });

export const bodyRaw = (body: unknown, contentType?: string, contentLength?: number): Body =>
  Object.freeze({ _tag: "Raw", body, contentType, contentLength });

const contentTypeOf = (body: Body): string | undefined =>
  body._tag === "Uint8Array" || body._tag === "Raw" ? body.contentType : undefined;
const contentLengthOf = (body: Body): number | undefined =>
  body._tag === "Uint8Array" || body._tag === "Raw" ? body.contentLength : undefined;

export const bodyContentType = (body: Body): Option<string> => {
  const type = contentTypeOf(body);
  return type === undefined ? none : some(type);
};

export const bodyContentLength = (body: Body): Option<number> => {
  const length = contentLengthOf(body);
  return length === undefined ? none : some(length);
};

/** `Http.ServerResponse.ServerResponse`. */
export class ServerResponse {
  readonly status: number;
  readonly statusText: string | undefined;
  readonly headers: Headers;
  readonly cookies: Cookies;
  readonly body: Body;
  constructor(status: number, statusText: string | undefined, headers: Headers, cookies: Cookies, body: Body) {
    this.status = status;
    this.statusText = statusText;
    this.headers = headers;
    this.cookies = cookies;
    this.body = body;
    Object.freeze(this);
  }
}

interface Parts {
  readonly status: number;
  readonly statusText?: string | undefined;
  readonly headers?: Headers | undefined;
  readonly cookies?: Cookies | undefined;
  readonly body?: Body | undefined;
}

/**
 * A response as Effect makes one: a body with a content type or length puts
 * them in the headers. `owned` headers replace those of `parts`, and their
 * own content headers win over the body's.
 */
export const makeResponse = (parts: Parts, owned?: Headers): ServerResponse => {
  const body = parts.body ?? bodyEmpty;
  const type = contentTypeOf(body);
  const length = contentLengthOf(body);
  let headers = owned ?? parts.headers ?? headersEmpty;
  if (body._tag !== "Empty" && (type || length !== undefined)) {
    const extra: Array<[string, string]> = [];
    if (type && (owned === undefined || headers["content-type"] === undefined)) extra.push(["content-type", type]);
    if (length !== undefined && (owned === undefined || headers["content-length"] === undefined)) extra.push(["content-length", String(length)]);
    headers = fromInput([...Object.entries(headers), ...extra]);
  }
  return new ServerResponse(parts.status, parts.statusText, headers, parts.cookies ?? cookiesEmpty, body);
};

/** The headers a new body leaves: its content type and length, or none. */
export const updateHeaders = (headers: Headers, body: Body): Headers => {
  const type = contentTypeOf(body);
  const length = contentLengthOf(body);
  const kept = Object.entries(headers).filter(([n]) => !(n === "content-type" && type === undefined) && !(n === "content-length" && length === undefined));
  const set: Array<[string, string]> = [];
  if (type !== undefined) set.push(["content-type", type]);
  if (length !== undefined) set.push(["content-length", String(length)]);
  return fromInput([...kept, ...set]);
};

/** The options of a new response as Effect takes them: a field left out sets nothing. */
export interface ResponseOptions {
  readonly status?: number | undefined;
  readonly statusText?: string | undefined;
  readonly headers?: ReadonlyArray<readonly [string, string]> | undefined;
  readonly cookies?: Cookies | undefined;
  readonly contentType?: string | undefined;
}

/** A response without a body: 204 unless the options set a status. */
export const emptyResponse = (o: ResponseOptions = {}): ServerResponse =>
  makeResponse({
    status: o.status ?? 204,
    statusText: o.statusText,
    headers: o.headers ? fromInput(o.headers) : undefined,
    cookies: o.cookies,
  });

/** A response of the body that `make` builds of the content type the options choose. */
export const bodyResponse = (o: ResponseOptions, make: (contentType: string | undefined) => Body): ServerResponse => {
  const headers = o.headers ? fromInput(o.headers) : headersEmpty;
  // Effect reads the content type of the headers only when the options give headers
  const contentType = o.contentType ? o.contentType : o.headers ? headers["content-type"] : undefined;
  return makeResponse({ status: o.status ?? 200, statusText: o.statusText, headers, cookies: o.cookies, body: make(contentType) });
};

export const redirectResponse = (location: string, o: ResponseOptions = {}): ServerResponse => {
  const headers = fromInput([["location", location]]);
  return makeResponse({
    status: o.status ?? 302,
    statusText: o.statusText,
    headers: o.headers ? merge(fromInput(o.headers))(headers) : headers,
    cookies: o.cookies,
  });
};

/** Whether the response goes without its body: for `HEAD`, or a status that has none. */
export const omitsBody = (response: ServerResponse, withoutBody = false): boolean =>
  withoutBody || response.status === 204 || response.status === 205 || response.status === 304;

const METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS", "TRACE", "QUERY"];
const METHOD_TAG = "Http.Method.Method";
const methodsByName = /*#__PURE__*/ new Map(METHODS.map((m) => [m, Object.freeze({ _tag: tag(`${METHOD_TAG}.${m}`) })]));
const namesByTag = /*#__PURE__*/ new Map(METHODS.map((m) => [tag(`${METHOD_TAG}.${m}`), m]));

/** `Http.Method.Method` of the name; another method is a defect, as with Effect. */
export const methodToFlick = (method: string): Readonly<{ _tag: string }> => {
  const value = methodsByName.get(method);
  if (value === undefined) throw new TypeError(`not a ${METHOD_TAG}: ${method}`);
  return value;
};

export const methodFromFlick = (method: Readonly<{ _tag: string }>): string => {
  const name = namesByTag.get(method._tag);
  if (name === undefined) throw new TypeError(`not a ${METHOD_TAG}: ${method._tag}`);
  return name;
};

const SERVER_ERROR = /*#__PURE__*/ tag("Http.ServerError.HttpServerError.HttpServerError");
const SERVE_ERROR = /*#__PURE__*/ tag("Http.ServerError.ServeError.ServeError");
const MULTIPART_ERROR = /*#__PURE__*/ tag("Http.Multipart.MultipartError.MultipartError");

type Kind = "RequestParseError" | "RouteNotFound" | "InternalError";

const STATUS = /*#__PURE__*/ new Map<string, number>([
  [tag("Http.ServerError.ServerErrorReason.RequestParseError"), 400],
  [tag("Http.ServerError.ServerErrorReason.RouteNotFound"), 404],
  [tag("Http.ServerError.ServerErrorReason.InternalError"), 500],
  [tag("Http.ServerError.ServerErrorReason.ResponseError"), 500],
  [tag("Http.Multipart.MultipartErrorReason.FileTooLarge"), 413],
  [tag("Http.Multipart.MultipartErrorReason.FieldTooLarge"), 413],
  [tag("Http.Multipart.MultipartErrorReason.BodyTooLarge"), 413],
  [tag("Http.Multipart.MultipartErrorReason.TooManyParts"), 413],
  [tag("Http.Multipart.MultipartErrorReason.InternalError"), 500],
  [tag("Http.Multipart.MultipartErrorReason.Parse"), 400],
]);

/** What went wrong, as the error's description: its own, or the message of its cause. */
const describe = (cause: unknown): Option<string> => {
  const text = cause instanceof Error ? cause.message : typeof cause === "string" ? cause : undefined;
  return text === undefined ? none : some(text);
};

/** The Flick `HttpServerError` of the request; the URL has its secrets masked. */
export const httpServerError = (kind: Kind, request: ServerRequest, cause?: unknown) => ({
  _tag: SERVER_ERROR,
  _0: {
    _tag: tag(`Http.ServerError.ServerErrorReason.${kind}`),
    _0: { method: methodToFlick(request.method), url: flickRedactUrl(redactedNames(), request.url), description: describe(cause) },
  },
});

/** The Flick `ServeError` of a server that could not listen. */
export const serveError = (cause: unknown) => ({ _tag: SERVE_ERROR, _0: cause instanceof Error ? cause.message : String(cause) });

interface Reasoned {
  readonly _tag: string;
  readonly _0: { readonly _tag: string };
}

/** The status a server answers a Flick `HttpServerError` or `MultipartError` with, or `undefined`. */
export const statusOf = (e: unknown): number | undefined => {
  if (typeof e !== "object" || e === null) return undefined;
  const value = e as Partial<Reasoned>;
  return value._tag === SERVER_ERROR || value._tag === MULTIPART_ERROR ? STATUS.get(value._0?._tag ?? "") : undefined;
};

/** The id that interrupts a request whose client went away: Effect annotates it as a client abort. */
export const CLIENT_ABORT = -499;

/**
 * The response to a failed handler, as the adapter answers it: the first
 * typed failure gets its status if it is a Flick `Http` error and 500
 * otherwise; with none, a response the handler died with is sent as it is,
 * another defect is 500, and an interruption is 499 when the client went
 * away and 503 when the server stops.
 */
export const causeResponse = (cause: Cause): ServerResponse => {
  const failure = cause.reasons.find(isFail);
  if (failure !== undefined) return emptyResponse({ status: statusOf(failure.error) ?? 500 });
  let response: ServerResponse | undefined;
  let died = false;
  let clientAbort = false;
  for (const reason of cause.reasons) {
    if (reason._tag === "Die") {
      if (reason.defect instanceof ServerResponse) response = reason.defect;
      else died = true;
    } else if (reason._tag === "Interrupt") {
      clientAbort = reason.fiberId === CLIENT_ABORT;
    }
  }
  if (response !== undefined) return response;
  if (died || cause.reasons.length === 0) return emptyResponse({ status: 500 });
  return emptyResponse({ status: clientAbort ? 499 : 503 });
};
