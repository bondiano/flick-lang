// `HttpApi.call`: a request of an endpoint from the same value the server
// is built from. Its parts are encoded with the endpoint's codecs, the
// response decoded by its status: the success, a declared error as the
// typed failure, anything else a `ClientError`.
import { Effect, Exit, Option, Redacted, Result, Stream } from "effect";

import type { Tagged } from "../data.ts";
import { codecDecode, codecEncode, codecEncodeResult } from "../schema/index.ts";
import type { AnySchema } from "../schema/schemas.ts";
import { tag } from "../tags.ts";
import { strings } from "./encodings.ts";
import { type Endpoint, hasPayload, typed } from "./model.ts";

/** `HttpApi.ClientError`. */
export type ClientError = Tagged<string, { readonly status: number; readonly message: string }>;

export const clientError = (status: number, message: string): ClientError => ({
  _tag: tag("HttpApi.ClientError.ClientError"),
  _0: { status, message },
});

export interface Request {
  readonly path: unknown;
  readonly query: unknown;
  readonly headers: unknown;
  readonly payload: unknown;
}

const encode = (codec: AnySchema | null, value: unknown, part: string): Effect.Effect<unknown, ClientError, unknown> =>
  codec === null
    ? Effect.succeed(undefined)
    : Effect.mapError(codecEncode(codec, value), (e) =>
        clientError(0, `${part}: ${e._0.map((i) => `${i.path}: ${i.message}`).join("; ")}`),
      );

const json = (text: string): unknown => {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return text;
  }
};

/** The decoded value of the first declared error of this status that decodes. */
const declared = (endpoint: Endpoint, status: number, body: unknown): Effect.Effect<unknown, unknown, unknown> =>
  Effect.gen(function* () {
    for (const e of endpoint.errors.filter((c) => c.status === status)) {
      const exit = yield* Effect.exit(codecDecode(e.codec, body));
      if (Exit.isSuccess(exit)) return yield* Effect.fail(exit.value);
    }
    return yield* Effect.fail(clientError(status, typeof body === "string" ? body : JSON.stringify(body)));
  });

const MEDIA: Readonly<Record<string, string>> = {
  json: "application/json",
  text: "text/plain",
  form: "application/x-www-form-urlencoded",
  bytes: "application/octet-stream",
};

/** A `FormData` of the fields of an encoded record; a list is a field of each value. */
const formOf = (encoded: unknown): FormData => {
  const data = new FormData();
  if (typeof encoded === "object" && encoded !== null) {
    for (const [k, v] of Object.entries(encoded)) {
      for (const one of Array.isArray(v) ? v : [v]) if (one !== undefined && one !== null) data.append(k, typeof one === "string" ? one : JSON.stringify(one));
    }
  }
  return data;
};

/** `Http.Multipart.Part` as Flick holds it. */
interface FlickPart {
  readonly _tag: string;
  readonly _0: {
    readonly key: string;
    readonly contentType: string;
    readonly value?: string;
    readonly name?: string;
    readonly content?: Stream.Stream<ReadonlyArray<number>, unknown>;
  };
}

/** A `FormData` of the parts: each file's bytes are read. */
const partsForm = (parts: Stream.Stream<FlickPart, unknown>): Effect.Effect<FormData, ClientError> =>
  Effect.mapError(
    Stream.runFold(
      Stream.mapEffect(parts, (part) =>
        part._0.content === undefined
          ? Effect.succeed([part, new Uint8Array()] as const)
          : Effect.map(Stream.runCollect(part._0.content), (chunks) => [part, Uint8Array.from(chunks.flat())] as const),
      ),
      () => new FormData(),
      (data, [part, bytes]) => {
        if (part._0.content === undefined) data.append(part._0.key, part._0.value ?? "");
        else data.append(part._0.key, new Blob([bytes], { type: part._0.contentType }), part._0.name ?? part._0.key);
        return data;
      },
    ),
    (e) => clientError(0, `payload: ${String(e)}`),
  );

/** The body a call sends, and its content type (a form's own boundary for `FormData`). */
const sent = (endpoint: Endpoint, payload: unknown): Effect.Effect<BodyInit | undefined, ClientError> => {
  switch (endpoint.payloadEncoding) {
    case "json":
      return Effect.succeed(JSON.stringify(payload));
    case "text":
      return Effect.succeed(String(payload));
    case "form":
      return Effect.succeed(new URLSearchParams(strings(payload)).toString());
    case "multipart":
      return Effect.succeed(formOf(payload));
    case "bytes":
      return Effect.succeed(Uint8Array.from(typed<ReadonlyArray<number>>(payload)));
    default:
      return partsForm(typed<Stream.Stream<FlickPart, unknown>>(payload));
  }
};

const undecoded = (status: number, issues: { readonly _0: ReadonlyArray<{ readonly path: string; readonly message: string }> }) =>
  clientError(status, `the response does not decode: ${issues._0.map((i) => `${i.path}: ${i.message}`).join("; ")}`);

/** The SSE events of the text: their id, name and data lines. */
const events = (text: string) =>
  text
    .split(/\r?\n\r?\n/)
    .map((block) => {
      const fields = block.split(/\r?\n/).map((line) => [line.slice(0, line.indexOf(":")), line.slice(line.indexOf(":") + 1).replace(/^ /, "")] as const);
      const data = fields.filter(([k]) => k === "data").map(([, v]) => v);
      const id = fields.find(([k]) => k === "id");
      const name = fields.find(([k]) => k === "event");
      return { id: Option.fromUndefinedOr(id?.[1]), event: name?.[1] ?? "message", data: data.join("\n"), any: data.length > 0 };
    })
    .filter((e) => e.any)
    .map((e) => ({ id: e.id, event: e.event, data: e.data }));

export interface Received {
  readonly status: number;
  readonly headers: Headers;
  readonly bytes: Uint8Array;
}

/** The success value of the response, before its extras. */
const successOf = (endpoint: Endpoint, r: Received, text: string): Effect.Effect<unknown, ClientError, unknown> => {
  const success = endpoint.success;
  switch (endpoint.successEncoding) {
    case "bytes":
      return Effect.succeed(Array.from(r.bytes));
    case "stream":
      return Effect.succeed(Stream.succeed(Array.from(r.bytes)));
    case "events":
      return Effect.succeed(Stream.fromIterable(events(text)));
    case "sse":
      if (success === null) return Effect.void;
      return Effect.map(
        Effect.forEach(events(text), (e) => Effect.mapError(codecDecode(success, json(e.data)), (x) => undecoded(r.status, x))),
        Stream.fromIterable,
      );
    default:
      if (success === null) return Effect.void;
      return Effect.mapError(codecDecode(success, endpoint.successEncoding === "text" ? text : json(text)), (x) => undecoded(r.status, x));
  }
};

/** The value with each extra of the response: its decoded headers, the value of its cookie. */
const withExtras = (endpoint: Endpoint, r: Received, value: unknown): Effect.Effect<unknown, ClientError, unknown> =>
  Effect.reduce(endpoint.successExtras, () => value, (inner, extra) => {
    if (extra.kind === "headers") {
      return Effect.mapError(
        Effect.map(codecDecode(extra.codec, Object.fromEntries(r.headers), "ignore"), (h) => [inner, h] as const),
        (x) => undecoded(r.status, x),
      );
    }
    // a browser hides `Set-Cookie`, and an HttpOnly cookie, from scripts: the call still succeeds
    const cookie = r.headers.getSetCookie().map((c) => c.split(";")[0] ?? "").find((c) => c.startsWith(`${extra.key}=`));
    const shown = Option.map(Option.fromUndefinedOr(cookie), (c) => Redacted.make(decodeURIComponent(c.slice(extra.key.length + 1))));
    return Effect.succeed([inner, shown] as const);
  });

/** The address the browser shows, against which a relative base resolves. */
const pageAddress = (): string | undefined => {
  const location: unknown = Reflect.get(globalThis, "location");
  return typeof location === "object" && location !== null ? String(Reflect.get(location, "href")) : undefined;
};

/** `base` as a directory URL: `/api` in a browser is the page's origin's. */
const baseUrl = (base: string): Result.Result<URL, ClientError> => {
  const directory = base.endsWith("/") ? base : `${base}/`;
  try {
    return Result.succeed(new URL(directory, pageAddress()));
  } catch {
    return Result.fail(clientError(0, `the base ${JSON.stringify(base)} is no absolute URL, and there is no page to resolve it against`));
  }
};

/** How a request reaches a server: over the network, or to a handler. */
export type Send = (url: URL, init: RequestInit) => Effect.Effect<Received, ClientError, unknown>;

/** The request over the network with `fetch`. */
const fetched: Send = (url, init) =>
  Effect.tryPromise({
    try: async (signal) => {
      const r = await fetch(url, { ...init, signal });
      return { status: r.status, headers: r.headers, bytes: new Uint8Array(await r.arrayBuffer()) };
    },
    catch: (error) => clientError(0, error instanceof Error ? error.message : String(error)),
  });

/** A request of `endpoint` under `root`, sent by `send`, its response decoded. */
export const callWith = (send: Send, root: URL, endpoint: Endpoint, request: Request): Effect.Effect<unknown, unknown, unknown> =>
  Effect.gen(function* () {
    const params = new Map(strings(yield* encode(endpoint.params, request.path, "path")));
    const path = endpoint.path.replace(/:([A-Za-z_][\w]*)/g, (_, name: string) => encodeURIComponent(params.get(name) ?? ""));
    const url = new URL(path.replace(/^\//, ""), root);
    for (const [k, v] of strings(yield* encode(endpoint.query, request.query, "query"))) url.searchParams.append(k, v);
    const headers = new Headers(strings(yield* encode(endpoint.headers, request.headers, "headers")));
    const payload = endpoint.payload === null ? request.payload : yield* encode(endpoint.payload, request.payload, "payload");
    const type = MEDIA[endpoint.payloadEncoding];
    if (hasPayload(endpoint) && type !== undefined) headers.set("content-type", type);
    const body = hasPayload(endpoint) ? yield* sent(endpoint, payload) : undefined;
    const response = yield* send(url, { method: endpoint.method, headers, ...(body === undefined ? {} : { body }) });
    const text = new TextDecoder().decode(response.bytes);
    if (response.status !== endpoint.status) return yield* declared(endpoint, response.status, json(text));
    return yield* withExtras(endpoint, response, yield* successOf(endpoint, response, text));
  });

/** `HttpApi.call base endpoint request`. */
export const httpApiCall = (base: string, endpoint: Endpoint, request: Request): Effect.Effect<unknown, unknown, unknown> =>
  Effect.flatMap(Effect.fromResult(baseUrl(base)), (root) => callWith(fetched, root, endpoint, request));

/** The fields of an encoded part as strings, or the error of its encoding. */
const encodedPart = (codec: AnySchema | null, value: unknown, name: string): Result.Result<Array<[string, string]>, ClientError> => {
  if (codec === null) return Result.succeed([]);
  const encoded = codecEncodeResult(codec, value);
  return Result.isSuccess(encoded)
    ? Result.succeed(strings(encoded.success))
    : Result.fail(clientError(0, `${name}: ${encoded.failure._0.map((i) => `${i.path}: ${i.message}`).join("; ")}`));
};

/** `HttpApi.url base endpoint path query`: the URL a call would request. */
export const httpApiUrl = (base: string, endpoint: Endpoint, path: unknown, query: unknown): Result.Result<string, ClientError> => {
  const params = encodedPart(endpoint.params, path, "path");
  if (Result.isFailure(params)) return Result.fail(params.failure);
  const search = encodedPart(endpoint.query, query, "query");
  if (Result.isFailure(search)) return Result.fail(search.failure);
  const values = new Map(params.success);
  const resolved = endpoint.path.replace(/:([A-Za-z_][\w]*)/g, (_, name: string) => encodeURIComponent(values.get(name) ?? ""));
  const root = baseUrl(base);
  if (Result.isFailure(root)) return Result.fail(root.failure);
  const url = new URL(resolved.replace(/^\//, ""), root.success);
  for (const [k, v] of search.success) url.searchParams.append(k, v);
  return Result.succeed(url.href);
};
