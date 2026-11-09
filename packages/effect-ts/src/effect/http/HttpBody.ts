// Effect's `HttpBody` for the bindings of the standard library: bytes as
// lists of numbers, and `HttpBodyError` as the Flick error `Http.Body.HttpBodyError`.
import { Effect, FileSystem, Option, SchemaIssue, Stream } from "effect";
import * as HttpBody from "effect/http/HttpBody";

import { type AnySchema, codecEncode } from "../../schema/index.ts";
import { tag } from "@flick-lang/runtime/tags";
import { effectFileSystem, effectInfo, type info } from "../FileSystem.ts";
import { mapError as platformError } from "../PlatformError.ts";
import { toBytes } from "./convert.ts";

export * from "effect/http/HttpBody";

const ERROR = /*#__PURE__*/ tag("Http.Body.HttpBodyError.HttpBodyError");
const INVALID_JSON = /*#__PURE__*/ tag("Http.Body.BodyErrorReason.InvalidJson");
const INVALID_SCHEMA = /*#__PURE__*/ tag("Http.Body.BodyErrorReason.InvalidSchema");

const formatIssue = /*#__PURE__*/ SchemaIssue.makeFormatterDefault();

/** The Flick value of an Effect `HttpBodyError`. */
export const toFlick = (error: HttpBody.HttpBodyError) => ({
  _tag: ERROR,
  _0:
    error.reason._tag === "JsonError" ? { _tag: INVALID_JSON } : { _tag: INVALID_SCHEMA, _0: formatIssue(error.reason.issue) },
});

/** Typed failures of `effect` that are an Effect `HttpBodyError` become the Flick error. */
export const mapError = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
  Effect.mapError(effect, (e) => (HttpBody.isHttpBodyError(e) ? toFlick(e) : e));

/** `Http.Body.uint8_array bytes contentType`. */
export const flickUint8Array = (bytes: ReadonlyArray<number>, contentType: Option.Option<string>) =>
  HttpBody.uint8Array(toBytes(bytes), Option.getOrUndefined(contentType));

/** `Http.Body.json value`. */
export const flickJson = (value: unknown) => mapError(HttpBody.json(value));

/** `Http.Body.json_schema codec value`: the value encoded, then as JSON. */
export const flickJsonSchema = (codec: AnySchema, value: unknown) =>
  Effect.flatMap(codecEncode(codec, value), flickJson);

/** A `FormData` of the pairs. */
export const formData = (pairs: ReadonlyArray<readonly [string, string]>): FormData => {
  const data = new FormData();
  for (const [k, v] of pairs) data.append(k, v);
  return data;
};

/** `Http.Body.form_data pairs`. */
export const flickFormData = (pairs: ReadonlyArray<readonly [string, string]>) => HttpBody.formData(formData(pairs));

export const flickContentType = (body: HttpBody.HttpBody): Option.Option<string> =>
  body._tag === "Empty" ? Option.none() : Option.fromUndefinedOr(body.contentType);

export const flickContentLength = (body: HttpBody.HttpBody): Option.Option<number> =>
  body._tag === "Empty" ? Option.none() : Option.fromUndefinedOr(body.contentLength);

/** The text of a body of bytes. */
export const flickToText = (body: HttpBody.HttpBody): Option.Option<string> =>
  body._tag === "Uint8Array" ? Option.some(body.text ?? new TextDecoder().decode(body.body)) : Option.none();

/** `Http.Body.stream chunks contentType contentLength`. */
export const flickStream = (
  chunks: Stream.Stream<ReadonlyArray<number>, unknown>,
  contentType: Option.Option<string>,
  contentLength: Option.Option<number>,
) => HttpBody.stream(Stream.map(chunks, toBytes), Option.getOrUndefined(contentType), Option.getOrUndefined(contentLength));

/** `Http.Body.FileOptions`. */
export interface FileOptions {
  readonly offset: Option.Option<bigint>;
  readonly bytesToRead: Option.Option<bigint>;
  readonly chunkSize: Option.Option<number>;
  readonly contentType: Option.Option<string>;
}

/** `Http.Body.file_from_info options path info`, read through the `open` of the Flick file system. */
export const flickFileFromInfo = (o: FileOptions, path: string, i: ReturnType<typeof info>) =>
  Effect.flatMap(effectFileSystem, (fs) =>
    platformError(
      Effect.provideService(
        HttpBody.fileFromInfo(path, effectInfo(i), {
          offset: Option.getOrUndefined(o.offset),
          bytesToRead: Option.getOrUndefined(o.bytesToRead),
          chunkSize: Option.getOrUndefined(o.chunkSize),
          contentType: Option.getOrUndefined(o.contentType),
        }),
        FileSystem.FileSystem,
        FileSystem.make(fs),
      ),
    ),
  );
