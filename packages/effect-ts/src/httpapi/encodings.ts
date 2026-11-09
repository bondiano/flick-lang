// The bodies of `HttpApi` endpoints besides JSON, text and URL-encoded
// forms: bytes, multipart forms whole or part by part, streams of bytes,
// Server-Sent Events, and the response headers and cookie a handler
// answers with its value. The server reads and writes them here; the
// client sends and reads them in `client.ts`.
import { Effect, Option, Redacted, Stream } from "effect";
import { type HttpServerRequest, HttpServerResponse } from "effect/http";

import { flickParts, flickPersisted, flickSchemaPersisted } from "../effect/http/Multipart.ts";
import { codecEncode } from "../schema/index.ts";
import { type Endpoint, type SuccessExtra, typed } from "./model.ts";

type Response = HttpServerResponse.HttpServerResponse;

/** The fields of an encoded record as strings; `null` and absent fields are left out. */
export const strings = (encoded: unknown): Array<[string, string]> =>
  typeof encoded === "object" && encoded !== null
    ? Object.entries(encoded).flatMap(([k, v]: [string, unknown]) =>
        v === undefined || v === null ? [] : [[k, typeof v === "string" ? v : JSON.stringify(v)] as [string, string]],
      )
    : [];

/**
 * The payload of a body that is no JSON, text or URL-encoded form: bytes,
 * the stream of a form's parts, or a multipart form decoded by the codec.
 */
export const otherPayload = (endpoint: Endpoint, request: HttpServerRequest.HttpServerRequest): Effect.Effect<unknown, unknown, unknown> => {
  if (endpoint.payloadEncoding === "bytes") return Effect.map(Effect.orElseSucceed(request.arrayBuffer, () => new ArrayBuffer(0)), (b) => Array.from(new Uint8Array(b)));
  if (endpoint.payloadEncoding === "parts") return Effect.succeed(flickParts(request));
  const codec = endpoint.payload;
  return Effect.flatMap(flickPersisted(request), (persisted) => (codec === null ? Effect.void : flickSchemaPersisted(codec, persisted)));
};

/** An SSE event of the lines of its fields. */
const event = (fields: ReadonlyArray<readonly [string, string]>): string =>
  `${fields.flatMap(([k, v]) => v.split("\n").map((line) => `${k}: ${line}`)).join("\n")}\n\n`;

interface FlickEvent {
  readonly id: Option.Option<string>;
  readonly event: string;
  readonly data: string;
}

const eventText = (e: FlickEvent): string =>
  event([...(Option.isSome(e.id) ? [["id", e.id.value] as const] : []), ["event", e.event], ["data", e.data]]);

const sse = (status: number, text: Stream.Stream<string, unknown>): Response =>
  HttpServerResponse.stream(Stream.encodeText(text), { status, contentType: "text/event-stream" });

/** The response of a success of bytes, a stream or events; `undefined` for another encoding. */
export const otherSuccess = (endpoint: Endpoint, value: unknown): Response | undefined => {
  const status = endpoint.status;
  switch (endpoint.successEncoding) {
    case "bytes":
      return HttpServerResponse.uint8Array(Uint8Array.from(typed<ReadonlyArray<number>>(value)), { status });
    case "stream":
      return HttpServerResponse.stream(Stream.map(typed<Stream.Stream<ReadonlyArray<number>>>(value), (b) => Uint8Array.from(b)), { status });
    case "events":
      return sse(status, Stream.map(typed<Stream.Stream<FlickEvent>>(value), eventText));
    case "sse": {
      const codec = endpoint.success;
      if (codec === null) return undefined;
      // the success codec of an endpoint is a `PureCodec`, which needs no services
      const data = Stream.mapEffect(typed<Stream.Stream<unknown>>(value), (v) =>
        Effect.map(typed<Effect.Effect<unknown, unknown>>(codecEncode(codec, v)), (encoded) => event([["data", JSON.stringify(encoded)]])),
      );
      return sse(status, data);
    }
    default:
      return undefined;
  }
};

/**
 * The handler's value without its extras, and what they add to the
 * response: the encoded headers and the cookie, Secure and HttpOnly.
 */
export const unwrapExtras = (
  extras: ReadonlyArray<SuccessExtra>,
  value: unknown,
): Effect.Effect<readonly [unknown, (response: Response) => Effect.Effect<Response, unknown>], unknown, unknown> =>
  Effect.gen(function* () {
    let inner = value;
    const steps: Array<(response: Response) => Effect.Effect<Response, unknown>> = [];
    for (const extra of extras.toReversed()) {
      const [v, added] = typed<readonly [unknown, unknown]>(inner);
      inner = v;
      if (extra.kind === "headers") {
        const headers = strings(yield* codecEncode(extra.codec, added));
        steps.push((r) => Effect.succeed(HttpServerResponse.setHeaders(r, headers)));
      } else {
        const cookie = typed<Option.Option<Redacted.Redacted>>(added);
        if (Option.isSome(cookie)) {
          steps.push((r) => HttpServerResponse.setCookie(r, extra.key, Redacted.value(cookie.value), { secure: true, httpOnly: true }));
        }
      }
    }
    const apply = (response: Response) =>
      steps.reduce<Effect.Effect<Response, unknown>>((acc, step) => Effect.flatMap(acc, step), Effect.succeed(response));
    return [inner, apply] as const;
  });
