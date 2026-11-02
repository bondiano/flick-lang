// URLs of a type that derives `Url`: a constructor is a path segment, its
// fields the segments after it, and a final record field the query. A segment
// is a scalar, or a constructor without fields written as a word; the last
// may be an `Option`, absent from the URL for `None`. A parsed value is
// checked by the type's own schema, so refined fields stay valid.
import { Option, Result } from "effect";

import { type AnySchema, schemaValidate } from "./schema/index.ts";

/** A query field: its label, its schema, and whether it is an `Option`. */
type QueryField = readonly [label: string, schema: AnySchema, optional: boolean];

const WORDS: unique symbol = Symbol("flick/route-words");

/** A segment of constructors without fields: each word with its tag. */
interface Words {
  readonly [WORDS]: ReadonlyArray<readonly [word: string, tag: string]>;
}

/** A segment: a scalar field's schema, or the words of a type's constructors. */
type Segment = AnySchema | Words;

/**
 * A constructor: its tag, its path segment ("" for the root), its segment
 * fields, its query, and whether its last segment is an `Option`.
 */
type Route = readonly [
  tag: string,
  path: string,
  segments: ReadonlyArray<Segment>,
  query: ReadonlyArray<QueryField> | null,
  optional: boolean,
];

/** `Url a`: what `Route.print` and `Route.parse` read. */
export interface UrlCodec {
  readonly schema: AnySchema;
  readonly routes: ReadonlyArray<Route>;
}

/** `T$url` of a type that derives `Url`. */
export const routeCodec = (schema: AnySchema, routes: ReadonlyArray<Route>): UrlCodec =>
  Object.freeze({ schema, routes });

/** A segment of the constructors with these words and tags. */
export const routeWords = (words: ReadonlyArray<readonly [string, string]>): Words => Object.freeze({ [WORDS]: words });

const isWords = (segment: Segment): segment is Words => typeof segment === "object" && segment !== null && WORDS in segment;

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

/** The text of a segment's field. */
const segmentText = (segment: Segment, field: unknown): string => {
  if (!isWords(segment)) return String(field);
  const found = segment[WORDS].find(([, tag]) => tag === Reflect.get(Object(field), "_tag"));
  if (found === undefined) throw new TypeError("a constructor of another type");
  return found[0];
};

/** The field of a segment's text. */
const segmentField = (segment: Segment, text: string): Option.Option<unknown> => {
  if (!isWords(segment)) return coerce(segment, text);
  const found = segment[WORDS].find(([word]) => word === text);
  return found === undefined ? Option.none() : Option.some({ _tag: found[1] });
};

/** `Route.print url value`: the path and query of `value`. */
export const routePrint = (codec: UrlCodec, value: Fields): string => {
  const route = codec.routes.find(([tag]) => tag === value["_tag"]);
  if (route === undefined) throw new TypeError("a value of another type");
  const [, path, segments, query, optional] = route;
  const texts = segments.flatMap((segment, i) => {
    const field = value[`_${i}`];
    if (!optional || i < segments.length - 1) return [segmentText(segment, field)];
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked the last segment is an `Option`
    const option = field as Option.Option<unknown>;
    return Option.isSome(option) ? [segmentText(segment, option.value)] : [];
  });
  const parts = [...(path === "" ? [] : [path]), ...texts];
  // a lone surrogate has no UTF-8 form: it prints as U+FFFD
  const url = `/${parts.map((p) => encodeURIComponent(p.toWellFormed())).join("/")}`;
  if (query === null) return url;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked the last field is a record
  const record = value[`_${segments.length}`] as Fields;
  const params = new URLSearchParams();
  for (const [label, , maybe] of query) {
    const field = record[label];
    if (!maybe) params.set(label, String(field).toWellFormed());
    else if (Option.isOption(field) && Option.isSome(field)) params.set(label, String(field.value).toWellFormed());
  }
  const search = params.toString();
  return search === "" ? url : `${url}?${search}`;
};

/** The segment fields of a route from the segments after its path, if they match. */
const segmentFields = (segments: ReadonlyArray<Segment>, optional: boolean, rest: ReadonlyArray<string>): Option.Option<unknown[]> => {
  const missing = optional && rest.length === segments.length - 1;
  if (rest.length !== segments.length && !missing) return Option.none();
  const fields: unknown[] = [];
  for (const [i, segment] of segments.entries()) {
    const last = optional && i === segments.length - 1;
    if (last && missing) {
      fields.push(Option.none());
      continue;
    }
    const field = segmentField(segment, rest[i] ?? "");
    if (Option.isNone(field)) return Option.none();
    fields.push(last ? field : field.value);
  }
  return Option.some(fields);
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
  for (const [tag, path, segments, query, optional] of codec.routes) {
    const rest = path === "" ? parts : parts[0] === path ? parts.slice(1) : undefined;
    if (rest === undefined) continue;
    const fields = segmentFields(segments, optional, rest);
    if (Option.isNone(fields)) continue;
    const value: Record<string, unknown> = { _tag: tag };
    fields.value.forEach((field, i) => (value[`_${i}`] = field));
    if (query !== null) {
      const record: Record<string, unknown> = {};
      let matched = true;
      for (const [label, schema, maybe] of query) {
        const param = url.searchParams.get(label);
        const field = param === null ? Option.none() : coerce(schema, param);
        if (maybe) record[label] = field;
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
