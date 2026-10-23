// `HttpApi.call`: a request of an endpoint from the same value the server
// is built from. Its parts are encoded with the endpoint's codecs, the
// response decoded by its status: the success, a declared error as the
// typed failure, anything else a `ClientError`.
import { Effect, Exit, Result } from "effect";

import type { Tagged } from "../data.ts";
import { codecDecode, codecEncode, codecEncodeResult } from "../schema/index.ts";
import type { AnySchema } from "../schema/schemas.ts";
import { tag } from "../tags.ts";
import type { Endpoint } from "./model.ts";

/** `HttpApi.ClientError`. */
export type ClientError = Tagged<string, { readonly status: number; readonly message: string }>;

const clientError = (status: number, message: string): ClientError => ({
  _tag: tag("HttpApi.ClientError.ClientError"),
  _0: { status, message },
});

interface Request {
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

/** The fields of an encoded record as strings; `null` and absent fields are left out. */
const strings = (encoded: unknown): Array<[string, string]> =>
  typeof encoded === "object" && encoded !== null
    ? Object.entries(encoded).flatMap(([k, v]: [string, unknown]) =>
        v === undefined || v === null ? [] : [[k, typeof v === "string" ? v : JSON.stringify(v)] as [string, string]],
      )
    : [];

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

/** `HttpApi.call base endpoint request`. */
export const httpApiCall = (base: string, endpoint: Endpoint, request: Request): Effect.Effect<unknown, unknown, unknown> =>
  Effect.gen(function* () {
    const params = new Map(strings(yield* encode(endpoint.params, request.path, "path")));
    const path = endpoint.path.replace(/:([A-Za-z_][\w]*)/g, (_, name: string) => encodeURIComponent(params.get(name) ?? ""));
    const url = new URL(path.replace(/^\//, ""), base.endsWith("/") ? base : `${base}/`);
    for (const [k, v] of strings(yield* encode(endpoint.query, request.query, "query"))) url.searchParams.append(k, v);
    const headers = new Headers(strings(yield* encode(endpoint.headers, request.headers, "headers")));
    const payload = yield* encode(endpoint.payload, request.payload, "payload");
    const types = { json: "application/json", text: "text/plain", form: "application/x-www-form-urlencoded" };
    if (endpoint.payload !== null) headers.set("content-type", types[endpoint.payloadEncoding]);
    const sent =
      endpoint.payloadEncoding === "json"
        ? JSON.stringify(payload)
        : endpoint.payloadEncoding === "text"
          ? String(payload)
          : new URLSearchParams(strings(payload)).toString();
    const response = yield* Effect.tryPromise({
      try: async (signal) => {
        const r = await fetch(url, {
          method: endpoint.method,
          headers,
          signal,
          ...(endpoint.payload === null ? {} : { body: sent }),
        });
        return { status: r.status, text: await r.text() };
      },
      catch: (error) => clientError(0, error instanceof Error ? error.message : String(error)),
    });
    if (response.status !== endpoint.status) return yield* declared(endpoint, response.status, json(response.text));
    const success = endpoint.success;
    if (success === null) return undefined;
    const received = endpoint.successEncoding === "text" ? response.text : json(response.text);
    return yield* Effect.mapError(codecDecode(success, received), (e) =>
      clientError(response.status, `the response does not decode: ${e._0.map((i) => `${i.path}: ${i.message}`).join("; ")}`),
    );
  });

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
  const url = new URL(resolved.replace(/^\//, ""), base.endsWith("/") ? base : `${base}/`);
  for (const [k, v] of search.success) url.searchParams.append(k, v);
  return Result.succeed(url.href);
};
