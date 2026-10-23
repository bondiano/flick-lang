// Effect's `Multipart` for the bindings of the standard library: forms of
// requests of the Node server, read with Node's file system, and
// `MultipartError` as the Flick error `Http.Multipart.MultipartError`.
import * as NodeServices from "@effect/platform-node/NodeServices";
import { Effect, Option } from "effect";
import type * as ByteSize from "effect/ByteSize";
import type * as Request from "effect/http/HttpServerRequest";
import * as Multipart from "effect/http/Multipart";
import * as MultipartParser from "effect/http/MultipartParser";

import { type AnySchema, codecDecode, jsonParse } from "../../schema/index.ts";
import { tag } from "../../tags.ts";
import { enumeration, toBytes } from "./convert.ts";
import { flickSchemaBodyUrlParams } from "./HttpServerRequest.ts";

export * from "effect/http/Multipart";

const reason = enumeration<Multipart.MultipartErrorReason["_tag"]>("Http.Multipart.MultipartErrorReason", [
  ["FileTooLarge", "FileTooLarge"],
  ["FieldTooLarge", "FieldTooLarge"],
  ["BodyTooLarge", "BodyTooLarge"],
  ["TooManyParts", "TooManyParts"],
  ["InternalError", "InternalError"],
  ["Parse", "Parse"],
]);

const ERROR = tag("Http.Multipart.MultipartError.MultipartError");
const STATUS = new Map<string, number>([
  [tag("Http.Multipart.MultipartErrorReason.FileTooLarge"), 413],
  [tag("Http.Multipart.MultipartErrorReason.FieldTooLarge"), 413],
  [tag("Http.Multipart.MultipartErrorReason.BodyTooLarge"), 413],
  [tag("Http.Multipart.MultipartErrorReason.TooManyParts"), 413],
  [tag("Http.Multipart.MultipartErrorReason.InternalError"), 500],
  [tag("Http.Multipart.MultipartErrorReason.Parse"), 400],
]);

/** The Flick value of an Effect `MultipartError`. */
export const toFlick = (error: Multipart.MultipartError) => ({ _tag: ERROR, _0: reason.flick(error.reason._tag) });

interface Flick {
  readonly _tag: string;
  readonly _0: { readonly _tag: string };
}

const isFlick = (e: unknown): e is Flick => typeof e === "object" && e !== null && "_tag" in e && e._tag === ERROR;

/** The status a server answers a Flick `MultipartError` with. */
export const multipartStatusOf = (e: unknown): number | undefined => (isFlick(e) ? STATUS.get(e._0._tag) : undefined);

/** `Http.Multipart.persisted request`. */
export const flickPersisted = (request: Request.HttpServerRequest) =>
  request.multipart.pipe(
    Effect.mapError((e) => (Multipart.isMultipartError(e) ? toFlick(e) : e)),
    Effect.provide(NodeServices.layer),
  );

const plainFile = (file: Multipart.PersistedFile) =>
  Object.freeze({ key: file.key, name: file.name, contentType: file.contentType, path: file.path });

const isFiles = (value: Multipart.Persisted[string]): value is ReadonlyArray<Multipart.PersistedFile> =>
  Array.isArray(value) && value.length > 0 && Multipart.isPersistedFile(value[0]);

export const flickFields = (persisted: Multipart.Persisted) =>
  Object.entries(persisted).flatMap(([k, v]) =>
    isFiles(v) ? [] : [[k, typeof v === "string" ? [v] : v] as const],
  );

export const flickFiles = (persisted: Multipart.Persisted) =>
  Object.entries(persisted).flatMap(([k, v]) => (isFiles(v) ? [[k, v.map(plainFile)] as const] : []));

const inert = (persisted: Multipart.Persisted): unknown =>
  Object.freeze(
    Object.fromEntries(
      Object.entries(persisted).map(([k, v]) => [
        k,
        isFiles(v) ? Object.freeze(v.map(plainFile)) : typeof v === "string" ? v : Object.freeze([...v]),
      ]),
    ),
  );

export const flickSchemaPersisted = (codec: AnySchema, persisted: Multipart.Persisted) =>
  codecDecode(codec, inert(persisted));

export const flickSchemaJson = (codec: AnySchema, field: string, persisted: Multipart.Persisted) =>
  Effect.suspend(() => {
    const value = persisted[field];
    const parsed = jsonParse(typeof value === "string" ? value : "");
    return parsed._tag === "Success" ? codecDecode(codec, parsed.success) : Effect.fail(parsed.failure);
  });

export const flickSchemaBodyMultipart = (codec: AnySchema, request: Request.HttpServerRequest) =>
  Effect.flatMap(flickPersisted(request), (persisted) => flickSchemaPersisted(codec, persisted));

export const flickSchemaBodyForm = (codec: AnySchema, request: Request.HttpServerRequest) =>
  (request.headers["content-type"] ?? "").toLowerCase().includes("multipart/form-data")
    ? flickSchemaBodyMultipart(codec, request)
    : flickSchemaBodyUrlParams(codec, request);

export const flickSchemaBodyFormJson = (codec: AnySchema, field: string, request: Request.HttpServerRequest) =>
  Effect.flatMap(flickPersisted(request), (persisted) => flickSchemaJson(codec, field, persisted));

/** `Http.Multipart.Limits`. */
export interface Limits {
  readonly maxParts: Option.Option<number>;
  readonly maxFieldSize: Option.Option<ByteSize.ByteSize>;
  readonly maxFileSize: Option.Option<ByteSize.ByteSize>;
  readonly maxTotalSize: Option.Option<ByteSize.ByteSize>;
  readonly fieldMimeTypes: ReadonlyArray<string>;
}

export const flickWithLimits = <A, E, R>(limits: Limits, effect: Effect.Effect<A, E, R>) =>
  Effect.provide(
    effect,
    Multipart.limitsServices({
      maxParts: Option.getOrUndefined(limits.maxParts),
      maxFieldSize: Option.getOrUndefined(limits.maxFieldSize),
      maxFileSize: Option.getOrUndefined(limits.maxFileSize),
      maxTotalSize: Option.getOrUndefined(limits.maxTotalSize),
      fieldMimeTypes: limits.fieldMimeTypes.length === 0 ? undefined : limits.fieldMimeTypes,
    }),
  );

/** `Http.Multipart.PartInfo`. */
export interface PartInfo {
  readonly name: string;
  readonly filename: Option.Option<string>;
  readonly contentType: string;
  readonly contentTypeParameters: ReadonlyArray<readonly [string, string]>;
  readonly contentDisposition: string;
  readonly contentDispositionParameters: ReadonlyArray<readonly [string, string]>;
  readonly headers: ReadonlyArray<readonly [string, string]>;
}

const partInfo = (info: PartInfo): MultipartParser.PartInfo => ({
  name: info.name,
  filename: Option.getOrUndefined(info.filename),
  contentType: info.contentType,
  contentTypeParameters: Object.fromEntries(info.contentTypeParameters),
  contentDisposition: info.contentDisposition,
  contentDispositionParameters: Object.fromEntries(info.contentDispositionParameters),
  headers: Object.fromEntries(info.headers),
});

export const flickDefaultIsFile = (info: PartInfo): boolean => MultipartParser.defaultIsFile(partInfo(info));

export const flickDecodeField = (info: PartInfo, bytes: ReadonlyArray<number>): string =>
  MultipartParser.decodeField(partInfo(info), toBytes(bytes));
