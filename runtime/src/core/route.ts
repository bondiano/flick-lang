// URLs of a type that derives `Url`: a constructor is a path segment, its
// fields the segments after it, and a final record field the query. A segment
// is a scalar, or a constructor without fields written as a word; the last
// may be an `Option`, absent from the URL for `None`, or a value of another
// route type, the rest of the path. A parsed value is checked by the type's
// own schema, so refined fields stay valid.
//
// One codec for both runtimes: each gives its schemas and its `Option`s.

/** An `Option` as either runtime builds it: both are read by `_tag`. */
export type Opt<A> = { readonly _tag: "None" } | { readonly _tag: "Some"; readonly value: A };

/** What a schema says of a value. */
export type Checked = { readonly _tag: "Failure" } | { readonly _tag: "Success"; readonly success: unknown };

/** What a runtime gives the codec: `S` its schemas, `O` its `Option` of any value. */
export interface RouteHost<S, O extends Opt<unknown>> {
  readonly none: () => O;
  readonly some: (value: unknown) => O;
  readonly validate: (schema: S, value: unknown) => Checked;
}

/** A query field: its label, its schema, and whether it is an `Option`. */
export type QueryField<S> = readonly [label: string, schema: S, optional: boolean];

const WORDS: unique symbol = Symbol("flick/route-words");

/** A segment of constructors without fields: each word with its tag. */
export interface Words {
  readonly [WORDS]: ReadonlyArray<readonly [word: string, tag: string]>;
}

const NESTED: unique symbol = Symbol("flick/route-nested");

/** The last segment of another route type: the rest of the path. */
export interface Nested<S> {
  readonly [NESTED]: () => UrlCodec<S>;
}

/** A segment: a scalar field's schema, the words of a type's constructors, or a nested route. */
export type Segment<S> = S | Words | Nested<S>;

/**
 * A constructor: its tag, its path segment ("" for the root), its segment
 * fields, its query, and whether its last segment is an `Option`.
 */
export type Route<S> = readonly [
  tag: string,
  path: string,
  segments: ReadonlyArray<Segment<S>>,
  query: ReadonlyArray<QueryField<S>> | null,
  optional: boolean,
];

/** `Url a`: what `Route.print` and `Route.parse` read. */
export interface UrlCodec<S> {
  readonly schema: S;
  readonly routes: ReadonlyArray<Route<S>>;
}

type Fields = Readonly<Record<string, unknown>>;

/** The tag of a constructor given as a value: a nullary one is its value, another a function of its fields. */
const tagOf = (constructor: unknown): string => {
  let value = constructor;
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- compiler-generated constructor stages
  while (typeof value === "function") value = (value as (field: unknown) => unknown)(undefined);
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- compiler-generated constructor data
  return (value as { readonly _tag: string })._tag;
};

const isOption = (v: unknown): v is Opt<unknown> =>
  typeof v === "object" && v !== null && (Reflect.get(v, "_tag") === "Some" || Reflect.get(v, "_tag") === "None");

const isWords = (segment: unknown): segment is Words => typeof segment === "object" && segment !== null && WORDS in segment;

const isNested = <S>(segment: Segment<S>): segment is Nested<S> =>
  typeof segment === "object" && segment !== null && NESTED in segment;

/** A segment of the constructors with these words and tags. */
const routeWords = (words: ReadonlyArray<readonly [string, string]>): Words => Object.freeze({ [WORDS]: words });

/** The codec of a runtime. */
export const routes = <S, O extends Opt<unknown>>(host: RouteHost<S, O>) => {
  /** `T$url` of a type that derives `Url`. */
  const routeCodec = (schema: S, of: ReadonlyArray<Route<S>>): UrlCodec<S> => Object.freeze({ schema, routes: of });

  /** The segment of a nested route type, its `T$url` read when first used. */
  const routeNested = (url: () => UrlCodec<S>): Nested<S> => Object.freeze({ [NESTED]: url });

  /** `Route.path c path segments query optional`: a constructor given as a value. */
  const routePath = (
    constructor: unknown,
    path: string,
    segments: ReadonlyArray<Segment<S>>,
    query: Opt<ReadonlyArray<QueryField<S>>>,
    optional: boolean,
  ): Route<S> => [tagOf(constructor), path, segments, query._tag === "Some" ? query.value : null, optional];

  /** `Route.segment schema`: a segment is the schema of its field. */
  const routeSegment = (schema: S): Segment<S> => schema;

  /** `Route.words words`: each word with its constructor. */
  const routeWordsOf = (words: ReadonlyArray<readonly [string, unknown]>): Words =>
    routeWords(words.map(([word, constructor]) => [word, tagOf(constructor)] as const));

  /** `Route.nested get`. */
  const routeNestedOf = (get: (unit: undefined) => UrlCodec<S>): Nested<S> => routeNested(() => get(undefined));

  /** `Route.query label schema optional`. */
  const routeQuery = (label: string, schema: S, optional: boolean): QueryField<S> => [label, schema, optional];

  /** Text as a value of `schema`: the text itself, else a number or a boolean. */
  const coerce = (schema: S, text: string): O => {
    const number = text.trim() === "" ? Number.NaN : Number(text);
    const candidates: ReadonlyArray<unknown> = [
      text,
      ...(Number.isNaN(number) && text !== "NaN" ? [] : [number]),
      ...(text === "true" ? [true] : text === "false" ? [false] : []),
    ];
    for (const candidate of candidates) {
      const result = host.validate(schema, candidate);
      if (result._tag === "Success") return host.some(result.success);
    }
    return host.none();
  };

  /** The texts of a segment's field: one, or a nested route's path. */
  const segmentTexts = (segment: Segment<S>, field: unknown): ReadonlyArray<string> => {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked the field is of the nested type
    if (isNested(segment)) return printParts(segment[NESTED](), field as Fields)[0];
    if (!isWords(segment)) return [String(field)];
    const found = segment[WORDS].find(([, tag]) => tag === Reflect.get(Object(field), "_tag"));
    if (found === undefined) throw new TypeError("a constructor of another type");
    return [found[0]];
  };

  /** The field of a segment's text. */
  const segmentField = (segment: S | Words, text: string): O => {
    if (!isWords(segment)) return coerce(segment, text);
    const found = segment[WORDS].find(([word]) => word === text);
    return found === undefined ? host.none() : host.some({ _tag: found[1] });
  };

  /** The path's segments of `value`, unencoded, and its route. */
  const printParts = (codec: UrlCodec<S>, value: Fields): readonly [ReadonlyArray<string>, Route<S>] => {
    const route = codec.routes.find(([tag]) => tag === value["_tag"]);
    if (route === undefined) throw new TypeError("a value of another type");
    const [, path, segments, , optional] = route;
    const texts = segments.flatMap((segment, i) => {
      const field = value[`_${i}`];
      if (!optional || i < segments.length - 1) return segmentTexts(segment, field);
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler checked the last segment is an `Option`
      const option = field as Opt<unknown>;
      return option._tag === "Some" ? segmentTexts(segment, option.value) : [];
    });
    return [[...(path === "" ? [] : [path]), ...texts], route];
  };

  /** `Route.print url value`: the path and query of `value`. */
  const routePrint = (codec: UrlCodec<S>, value: Fields): string => {
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
      else if (isOption(field) && field._tag === "Some") params.set(label, String(field.value).toWellFormed());
    }
    const search = params.toString();
    return search === "" ? url : `${url}?${search}`;
  };

  /** The segment fields of a route from the segments after its path, if they match. */
  const segmentFields = (
    segments: ReadonlyArray<Segment<S>>,
    optional: boolean,
    rest: ReadonlyArray<string>,
  ): unknown[] | undefined => {
    const last = segments.at(-1);
    if (last !== undefined && isNested(last)) {
      // the fixed segments, then the nested route takes the rest of the path
      const fixed = segments.length - 1;
      if (rest.length < fixed) return undefined;
      const fields = segmentFields(segments.slice(0, fixed), false, rest.slice(0, fixed));
      if (fields === undefined) return undefined;
      const tail = rest.slice(fixed);
      if (optional && tail.length === 0) return [...fields, host.none()];
      const nested = matchParts(last[NESTED](), tail, new URLSearchParams());
      if (nested === undefined) return undefined;
      return [...fields, optional ? host.some(nested) : nested];
    }
    const missing = optional && rest.length === segments.length - 1;
    if (rest.length !== segments.length && !missing) return undefined;
    const fields: unknown[] = [];
    for (const [i, segment] of segments.entries()) {
      const isLast = optional && i === segments.length - 1;
      if (isLast && missing) {
        fields.push(host.none());
        continue;
      }
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- only the last segment nests
      const field: Opt<unknown> = segmentField(segment as S | Words, rest[i] ?? "");
      if (field._tag === "None") return undefined;
      fields.push(isLast ? field : field.value);
    }
    return fields;
  };

  /** `Route.parse url text`: the value whose URL is `text`, ignoring its origin and fragment. */
  const routeParse = (codec: UrlCodec<S>, text: string): unknown => {
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
  const matchParts = (codec: UrlCodec<S>, parts: ReadonlyArray<string>, search: URLSearchParams): unknown => {
    for (const [tag, path, segments, query, optional] of codec.routes) {
      const rest = path === "" ? parts : parts[0] === path ? parts.slice(1) : undefined;
      if (rest === undefined) continue;
      const fields = segmentFields(segments, optional, rest);
      if (fields === undefined) continue;
      const value: Record<string, unknown> = { _tag: tag };
      fields.forEach((field, i) => (value[`_${i}`] = field));
      if (query !== null) {
        const record: Record<string, unknown> = {};
        let matched = true;
        for (const [label, schema, maybe] of query) {
          const param = search.get(label);
          const field: Opt<unknown> = param === null ? host.none() : coerce(schema, param);
          if (maybe) record[label] = field;
          else if (field._tag === "Some") record[label] = field.value;
          else matched = false;
        }
        if (!matched) continue;
        value[`_${segments.length}`] = record;
      }
      const checked = host.validate(codec.schema, value);
      if (checked._tag === "Success") return checked.success;
    }
    return undefined;
  };

  return {
    coerce,
    routeCodec,
    routeNested,
    routeNestedOf,
    routeParse,
    routePath,
    routePrint,
    routeQuery,
    routeSegment,
    routeWords,
    routeWordsOf,
  };
};
