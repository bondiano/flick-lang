// `Http.Body` on Flick's own runtime: a body is the `Body` of
// `../../http/model.ts`, as Effect's `HttpBody` would be. Streamed bodies
// and file bodies wait for Flick's streams and file system on this runtime,
// so a program with one takes every `Http` binding from the adapter.
import { type Option, none, some } from "../../../core/data.ts";
import { tag } from "../../../tags.ts";
import { fail, raise } from "../../cause.ts";
import type { Run } from "../../fiber.ts";
import { type Body, bodyBytes, bodyContentLength, bodyContentType, bodyEmpty, bodyFormData, bodyText } from "../../http/model.ts";
import { codecEncode } from "../../codec.ts";
import type { Witness } from "../../schema.ts";
import { type UrlParams, toString } from "./UrlParams.ts";

const ERROR = /*#__PURE__*/ tag("Http.Body.HttpBodyError.HttpBodyError");
const INVALID_JSON = /*#__PURE__*/ tag("Http.Body.BodyErrorReason.InvalidJson");

const optional = <A>(option: Option<A>): A | undefined => (option._tag === "Some" ? option.value : undefined);

/** The JSON text of the value; a value with no JSON text fails with `HttpBodyError InvalidJson`. */
export const jsonText = (value: unknown): string => {
  try {
    // no JSON text at all, as of `undefined`, is an empty body
    const text: string | undefined = JSON.stringify(value);
    return text ?? "";
  } catch {
    return raise(fail({ _tag: ERROR, _0: { _tag: INVALID_JSON } }));
  }
};

/** A `FormData` of the pairs. */
export const formData = (pairs: ReadonlyArray<readonly [string, string]>): FormData => {
  const data = new FormData();
  for (const [k, v] of pairs) data.append(k, v);
  return data;
};

export const empty: Body = bodyEmpty;

export const flickUint8Array = (bytes: ReadonlyArray<number>, contentType: Option<string>): Body =>
  bodyBytes(Uint8Array.from(bytes), optional(contentType));

export const text = (body: string, contentType?: string): Body => bodyText(body, contentType);

export const flickJson = (value: unknown): Body => bodyText(jsonText(value), "application/json");

export function* flickJsonSchema(codec: Witness, value: unknown): Run {
  return flickJson(yield codecEncode(codec, value));
}

export const urlParams = (params: UrlParams, contentType?: string): Body =>
  bodyText(toString(params), contentType ?? "application/x-www-form-urlencoded");

export const flickFormData = (pairs: ReadonlyArray<readonly [string, string]>): Body => bodyFormData(formData(pairs));

export const flickContentType = bodyContentType;

export const flickContentLength = bodyContentLength;

const decoder = /*#__PURE__*/ new TextDecoder();

/** The text of a body of bytes: a text, JSON or URL parameters body. */
export const flickToText = (body: Body): Option<string> =>
  body._tag === "Uint8Array" ? some(body.text ?? decoder.decode(body.body)) : none;
