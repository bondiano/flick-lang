// URLs of a type that derives `Url`: a constructor is a path segment, its
// fields the segments after it, and a final record field the query. A segment
// is a scalar, or a constructor without fields written as a word; the last
// may be an `Option`, absent from the URL for `None`, or a value of another
// route type, the rest of the path. A parsed value is checked by the type's
// own schema, so refined fields stay valid.
import { none, type Option as OptionValue, some } from "../core/data.ts";
import { type Witness, schemaValidate } from "./schema.ts";

const Option = {
  none: () => none,
  some,
  isSome: <A>(v: OptionValue<A>): v is { readonly _tag: "Some"; readonly value: A } => v._tag === "Some",
  isNone: <A>(v: OptionValue<A>): v is { readonly _tag: "None" } => v._tag === "None",
  isOption: (v: unknown): v is OptionValue<unknown> =>
    typeof v === "object" && v !== null && (Reflect.get(v, "_tag") === "Some" || Reflect.get(v, "_tag") === "None"),
  getOrNull: <A>(v: OptionValue<A>): A | null => v._tag === "Some" ? v.value : null,
};

const tagOf = (constructor: unknown): string => {
  let value = constructor;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- compiler-generated constructor stages
  while (typeof value === "function") value = (value as (field: unknown) => unknown)(undefined);
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- compiler-generated constructor data
  return (value as { readonly _tag: string })._tag;
};

/** A query field: its label, its schema, and whether it is an `Option`. */
type QueryField = readonly [label: string, schema: Witness, optional: boolean];

const WORDS: unique symbol = Symbol("flick/route-words");

/** A segment of constructors without fields: each word with its tag. */
interface Words {
  readonly [WORDS]: ReadonlyArray<readonly [word: string, tag: string]>;
}

const NESTED: unique symbol = Symbol("flick/route-nested");

/** The last segment of another route type: the rest of the path. */
interface Nested {
  readonly [NESTED]: () => UrlCodec;
}

/** A segment: a scalar field's schema, the words of a type's constructors, or a nested route. */
type Segment = Witness | Words | Nested;

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
  readonly schema: Witness;
  readonly routes: ReadonlyArray<Route>;
}

/** `T$url` of a type that derives `Url`. */
export const routeCodec = (schema: Witness, routes: ReadonlyArray<Route>): UrlCodec =>
  Object.freeze({ schema, routes });

/** A segment of the constructors with these words and tags. */
export const routeWords = (words: ReadonlyArray<readonly [string, string]>): Words => Object.freeze({ [WORDS]: words });

/** The segment of a nested route type, its `T$url` read when first used. */
export const routeNested = (url: () => UrlCodec): Nested => Object.freeze({ [NESTED]: url });

/** `Route.path c path segments query optional`: a constructor given as a value. */
export const routePath = (
  constructor: unknown,
  path: string,
  segments: ReadonlyArray<Segment>,
  query: OptionValue<ReadonlyArray<QueryField>>,
  optional: boolean,
): Route => [tagOf(constructor), path, segments, Option.getOrNull(query), optional];

/** `Route.segment schema`: a segment is the schema of its field. */
export const routeSegment = (schema: Witness): Segment => schema;

/** `Route.words words`: each word with its constructor. */
export const routeWordsOf = (words: ReadonlyArray<readonly [string, unknown]>): Words =>
  routeWords(words.map(([word, constructor]) => [word, tagOf(constructor)] as const));

/** `Route.nested get`. */
export const routeNestedOf = (get: (unit: undefined) => UrlCodec): Nested => routeNested(() => get(undefined));

/** `Route.query label schema optional`. */
export const routeQuery = (label: string, schema: Witness, optional: boolean): QueryField => [label, schema, optional];

const isWords = (segment: Segment): segment is Words => typeof segment === "object" && segment !== null && WORDS in segment;

const isNested = (segment: Segment): segment is Nested => typeof segment === "object" && segment !== null && NESTED in segment;

type Fields = Readonly<Record<string, unknown>>;

/** Text as a value of `schema`: the text itself, else a number or a boolean. */
export const coerce = (schema: Witness, text: string): OptionValue<unknown> => {
  const number = text.trim() === "" ? Number.NaN : Number(text);
  const candidates: ReadonlyArray<unknown> = [
    text,
    ...(Number.isNaN(number) && text !== "NaN" ? [] : [number]),
    ...(text === "true" ? [true] : text === "false" ? [false] : []),
  ];
  for (const candidate of candidates) {
    const result = schemaValidate(schema, candidate);
    if (result._tag === "Success") return Option.some(result.success);
  }
  return Option.none();
};

/** The texts of a segment's field: one, or a nested route's path. */
const segmentTexts = (segment: Segment, field: unknown): ReadonlyArray<string> => {
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked the field is of the nested type
  if (isNested(segment)) return printParts(segment[NESTED](), field as Fields)[0];
  if (!isWords(segment)) return [String(field)];
  const found = segment[WORDS].find(([, tag]) => tag === Reflect.get(Object(field), "_tag"));
  if (found === undefined) throw new TypeError("a constructor of another type");
  return [found[0]];
};

/** The field of a segment's text. */
const segmentField = (segment: Exclude<Segment, Nested>, text: string): OptionValue<unknown> => {
  if (!isWords(segment)) return coerce(segment, text);
  const found = segment[WORDS].find(([word]) => word === text);
  return found === undefined ? Option.none() : Option.some({ _tag: found[1] });
};

/** The path's segments of `value`, unencoded, and its route. */
const printParts = (codec: UrlCodec, value: Fields): readonly [ReadonlyArray<string>, Route] => {
  const route = codec.routes.find(([tag]) => tag === value["_tag"]);
  if (route === undefined) throw new TypeError("a value of another type");
  const [, path, segments, , optional] = route;
  const texts = segments.flatMap((segment, i) => {
    const field = value[`_${i}`];
    if (!optional || i < segments.length - 1) return segmentTexts(segment, field);
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked the last segment is an `Option`
    const option = field as OptionValue<unknown>;
    return Option.isSome(option) ? segmentTexts(segment, option.value) : [];
  });
  return [[...(path === "" ? [] : [path]), ...texts], route];
};

/** `Route.print url value`: the path and query of `value`. */
export const routePrint = (codec: UrlCodec, value: Fields): string => {
  const [parts, [, , segments, query]] = printParts(codec, value);
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
const segmentFields = (segments: ReadonlyArray<Segment>, optional: boolean, rest: ReadonlyArray<string>): OptionValue<unknown[]> => {
  const last = segments.at(-1);
  if (last !== undefined && isNested(last)) {
    // the fixed segments, then the nested route takes the rest of the path
    const fixed = segments.length - 1;
    if (rest.length < fixed) return Option.none();
    const fields = segmentFields(segments.slice(0, fixed), false, rest.slice(0, fixed));
    if (Option.isNone(fields)) return fields;
    const tail = rest.slice(fixed);
    if (optional && tail.length === 0) return Option.some([...fields.value, Option.none()]);
    const nested = matchParts(last[NESTED](), tail, new URLSearchParams());
    if (nested === undefined) return Option.none();
    return Option.some([...fields.value, optional ? Option.some(nested) : nested]);
  }
  const missing = optional && rest.length === segments.length - 1;
  if (rest.length !== segments.length && !missing) return Option.none();
  const fields: unknown[] = [];
  for (const [i, segment] of segments.entries()) {
    const isLast = optional && i === segments.length - 1;
    if (isLast && missing) {
      fields.push(Option.none());
      continue;
    }
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- only the last segment nests
    const field = segmentField(segment as Exclude<Segment, Nested>, rest[i] ?? "");
    if (Option.isNone(field)) return Option.none();
    fields.push(isLast ? field : field.value);
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
  return matchParts(codec, parts, url.searchParams);
};

/** The checked value of the first route matching these path segments and query. */
const matchParts = (codec: UrlCodec, parts: ReadonlyArray<string>, search: URLSearchParams): unknown => {
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
        const param = search.get(label);
        const field = param === null ? Option.none() : coerce(schema, param);
        if (maybe) record[label] = field;
        else if (Option.isSome(field)) record[label] = field.value;
        else matched = false;
      }
      if (!matched) continue;
      value[`_${segments.length}`] = record;
    }
    const checked = schemaValidate(codec.schema, value);
    if (checked._tag === "Success") return checked.success;
  }
  return undefined;
};
