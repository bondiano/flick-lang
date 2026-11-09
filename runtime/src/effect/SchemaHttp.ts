// Effect Schema's codecs of HTTP and web values for `Http.Codecs`: each is
// the canonical JSON codec Effect derives, and the codecs that decode into
// a codec of the program feed it the record or tree Effect builds. Their
// decoding fails on wire values of the right JSON type, so the strict
// exporter refuses them.
import { Effect, Schema, SchemaIssue, SchemaTransformation } from "effect";
import * as Cookies from "effect/http/Cookies";

import type { AnySchema } from "../schema/index.ts";
import { fallible } from "./Schema.ts";

const json = (schema: AnySchema): AnySchema => fallible(Schema.toCodecJson(schema));

const pairs = /*#__PURE__*/ Schema.Array(Schema.Tuple([Schema.String, Schema.String]));

export const flickHeaders = /*#__PURE__*/ json(Schema.Headers);
export const flickUrlParams = /*#__PURE__*/ json(Schema.UrlParams);
/** Effect gives a cookie no JSON form: here it is its `Set-Cookie` header text. */
export const flickCookie = /*#__PURE__*/ fallible(
  Schema.String.pipe(
    Schema.decodeTo(
      Schema.Cookie,
      SchemaTransformation.transformEffect({
        decode: (header: string, options) => {
          const [cookie] = Object.values(Cookies.fromSetCookie(header).cookies);
          return cookie === undefined
            ? Effect.fail(new SchemaIssue.InvalidValue({ expected: "a Set-Cookie header" }, header, options))
            : Effect.succeed(cookie);
        },
        encode: (cookie: Cookies.Cookie) => Effect.succeed(Cookies.serializeCookie(cookie)),
      }),
    ),
  ),
);
export const flickCookies = /*#__PURE__*/ json(Schema.Cookies);
export const flickUrl = /*#__PURE__*/ json(Schema.URLFromString);

/** The parameters as Effect's record of a value or values per key, decoded by `codec`. */
export const flickFromUrlParams = (codec: AnySchema): AnySchema =>
  json(Schema.RecordFromUrlParams.pipe(Schema.decodeTo(codec)));

/** The cookies as a record of their values, decoded by `codec`. */
export const flickFromCookies = (codec: AnySchema): AnySchema =>
  json(Schema.RecordFromCookies.pipe(Schema.decodeTo(codec)));

/** The JSON text of the first parameter `field`, decoded by `codec`. */
export const flickJsonUrlParamsField = (field: string, codec: AnySchema): AnySchema =>
  json(Schema.JsonFromUrlParamsField(field).pipe(Schema.decodeTo(codec)));

/** A query string, its bracket paths (`a[b]=1`) read as a tree decoded by `codec`. */
export const flickFromUrlSearchParams = (codec: AnySchema): AnySchema =>
  fallible(
    Schema.String.pipe(
      Schema.decodeTo(
        Schema.fromURLSearchParams(codec),
        SchemaTransformation.transform({
          decode: (query: string) => new URLSearchParams(query),
          encode: (params: URLSearchParams) => params.toString(),
        }),
      ),
    ),
  );

/** Form fields as name and value pairs, their bracket paths read as a tree decoded by `codec`. */
export const flickFromFormData = (codec: AnySchema): AnySchema =>
  fallible(
    pairs.pipe(
      Schema.decodeTo(
        Schema.fromFormData(codec),
        SchemaTransformation.transform({
          decode: (fields: ReadonlyArray<readonly [string, string]>) => {
            const data = new FormData();
            for (const [k, v] of fields) data.append(k, v);
            return data;
          },
          encode: (data: FormData): ReadonlyArray<readonly [string, string]> =>
            Array.from(data.entries(), ([k, v]) => [k, typeof v === "string" ? v : v.name] as const),
        }),
      ),
    ),
  );
