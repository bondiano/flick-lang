// URLs of a type that derives `Url`: a constructor is a path segment, its
// fields the segments after it, and a final record field the query. A parsed
// value is checked by the type's own schema, so refined fields stay valid.
import { Option, Result } from "effect";

import { type AnySchema, schemaValidate } from "./schema/index.ts";

/** A query field: its label, its schema, and whether it is an `Option`. */
type QueryField = readonly [label: string, schema: AnySchema, optional: boolean];

/** A constructor: its tag, its path segment ("" for the root), its segment fields and its query. */
type Route = readonly [tag: string, path: string, segments: ReadonlyArray<AnySchema>, query: ReadonlyArray<QueryField> | null];

/** `Url a`: what `Route.print` and `Route.parse` read. */
export interface UrlCodec {
  readonly schema: AnySchema;
  readonly routes: ReadonlyArray<Route>;
}

/** `T$url` of a type that derives `Url`. */
export const routeCodec = (schema: AnySchema, routes: ReadonlyArray<Route>): UrlCodec =>
  Object.freeze({ schema, routes });

type Fields = Readonly<Record<string, unknown>>;

/** Text as a value of `schema`: the text itself, else a number or a boolean. */
export const coerce = (schema: AnySchema, text: string): Option.Option<unknown> => {
  const number = text.trim() === "" ? Number.NaN : Number(text);
  const candidates: ReadonlyArray<unknown> = [
    text,
    ...(Number.isNaN(number) && text !== "NaN" ? [] : [number]),
    ...(text === "true" ? [true] : text === "false" ? [false] : []),
  ];
  for (const candidate of candidates) {
    const result = schemaValidate(schema, candidate);
    if (Result.isSuccess(result)) return Option.some(result.success);
  }
  return Option.none();
};

/** `Route.print url value`: the path and query of `value`. */
export const routePrint = (codec: UrlCodec, value: Fields): string => {
  const route = codec.routes.find(([tag]) => tag === value["_tag"]);
  if (route === undefined) throw new TypeError("a value of another type");
  const [, path, segments, query] = route;
  const parts = [...(path === "" ? [] : [path]), ...segments.map((_, i) => String(value[`_${i}`]))];
  // a lone surrogate has no UTF-8 form: it prints as U+FFFD
  const url = `/${parts.map((p) => encodeURIComponent(p.toWellFormed())).join("/")}`;
  if (query === null) return url;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked the last field is a record
  const record = value[`_${segments.length}`] as Fields;
  const params = new URLSearchParams();
  for (const [label, , optional] of query) {
    const field = record[label];
    if (!optional) params.set(label, String(field).toWellFormed());
    else if (Option.isOption(field) && Option.isSome(field)) params.set(label, String(field.value).toWellFormed());
  }
  const search = params.toString();
  return search === "" ? url : `${url}?${search}`;
};

/** `Route.parse url text`: the value whose URL is `text`, ignoring its origin and fragment. */
export const routeParse = (codec: UrlCodec, text: string): unknown => {
  let url: URL;
  try {
    url = new URL(text, "http://localhost");
  } catch {
    return undefined;
  }
  // "/" is the root; otherwise each "/" starts a segment, an empty one included
  const parts = (url.pathname === "/" ? [] : url.pathname.split("/").slice(1)).map((p) => {
    try {
      return decodeURIComponent(p);
    } catch {
      return p;
    }
  });
  for (const [tag, path, segments, query] of codec.routes) {
    const rest = path === "" ? parts : parts[0] === path ? parts.slice(1) : undefined;
    if (rest?.length !== segments.length) continue;
    const value: Record<string, unknown> = { _tag: tag };
    const fields = segments.map((schema, i) => coerce(schema, rest[i] ?? ""));
    if (!fields.every(Option.isSome)) continue;
    fields.forEach((field, i) => (value[`_${i}`] = field.value));
    if (query !== null) {
      const record: Record<string, unknown> = {};
      let matched = true;
      for (const [label, schema, optional] of query) {
        const param = url.searchParams.get(label);
        const field = param === null ? Option.none() : coerce(schema, param);
        if (optional) record[label] = field;
        else if (Option.isSome(field)) record[label] = field.value;
        else matched = false;
      }
      if (!matched) continue;
      value[`_${segments.length}`] = record;
    }
    const checked = schemaValidate(codec.schema, value);
    if (Result.isSuccess(checked)) return checked.success;
  }
  return undefined;
};
