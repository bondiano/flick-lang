// Effect's `Schema` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: codecs whose
// fallible decoding the strict exporter must refuse, records of `Option`
// fields, the program's `SchemaError` in place of Effect's issues, and
// Flick's own collections and `Ord` dictionaries.
import {
  Chunk,
  Duration,
  Effect,
  HashMap,
  HashSet,
  Option,
  Result,
  Schema,
  SchemaAST,
  SchemaGetter,
  type SchemaIssue,
  SchemaTransformation,
} from "effect";
import type { Order } from "effect";

import { EQ, LT, type HashDict, type OrdDict, type Ordering } from "../data.ts";
import { issueOf } from "../schema/codecs.ts";
import { type SchemaError, toSchemaError } from "../schema/issues.ts";
import type { Notes } from "../schema/notes.ts";
import { type AnySchema, jsonNumber, withJson, jsonOf } from "../schema/schemas.ts";
import {
  type FlickHashMap,
  type FlickHashSet,
  eqHashCanonical,
  hashMapInsert,
  hashMapToList,
  hashMapWith,
  hashSetFromList,
  hashSetSize,
  hashSetToList,
} from "../stdlib/hash.ts";

export * from "effect/Schema";

type Of<T> = Schema.Codec<T, unknown, unknown, unknown>;

/** `codec` with the runtime's notes, which `noteOf` reads back. */
const noted = (codec: AnySchema, notes: Notes): AnySchema => codec.annotate(notes);

/** A codec whose decoding may fail on a wire value of the right JSON type: no exact JSON Schema. */
export const fallible = (codec: AnySchema): AnySchema => noted(codec, { "flick/transform": "result" });

// Checks: Effect's filters as values, applied to a codec by `Check.apply`.

export const flickIsNonEmpty = Schema.isNonEmpty();
export const flickIsTrimmed = Schema.isTrimmed();
export const flickIsLowercased = Schema.isLowercased();
export const flickIsUppercased = Schema.isUppercased();
export const flickIsCapitalized = Schema.isCapitalized();
export const flickIsUncapitalized = Schema.isUncapitalized();
export const flickIsULID = Schema.isULID();
export const flickIsGUID = Schema.isGUID();
export const flickIsBase64 = Schema.isBase64();
export const flickIsBase64Url = Schema.isBase64Url();
export const flickIsStringFinite = Schema.isStringFinite();
export const flickIsStringBigInt = Schema.isStringBigInt();
export const flickIsInt = Schema.isInt();
export const flickIsInt32 = Schema.isInt32();
export const flickIsUint32 = Schema.isUint32();
export const flickIsFinite = Schema.isFinite();
export const flickIsUnique = Schema.isUnique();
export const flickIsUniqueKey = Schema.isUniqueKey();

/** `Check.pattern source`: a regular expression without flags; a malformed one is a defect. */
export const flickIsPattern = (source: string): SchemaAST.Filter<string> => Schema.isPattern(new RegExp(source));

/** `Check.predicate f message`: a check the program states as a function. */
export const flickMakeFilter = <T>(f: (value: T) => boolean, message: string): SchemaAST.Filter<T> =>
  Schema.makeFilter((value: T) => f(value), { message });

/** A check of the size of a Flick `HashSet`, through Effect's check of a `{ size }`. */
const sized = (check: SchemaAST.Filter<{ readonly size: number }>): SchemaAST.Filter<FlickHashSet<unknown>> =>
  new SchemaAST.Filter(
    (set: FlickHashSet<unknown>, self, options) => check.run({ size: hashSetSize(set) }, self, options),
    check.annotations,
  );

export const flickIsMinSize = (n: number) => sized(Schema.isMinSize(n));
export const flickIsMaxSize = (n: number) => sized(Schema.isMaxSize(n));
export const flickIsBetweenSize = (min: number, max: number) => sized(Schema.isBetweenSize(min, max));

/** Effect's `Order` of a Flick `Ord` dictionary. */
const orderOf =
  <T>(d: OrdDict<T>): Order.Order<T> =>
  (a, b) => {
    const o: Ordering = d.compare(a)(b);
    return o._tag === LT._tag ? -1 : o._tag === EQ._tag ? 0 : 1;
  };

export const flickMakeIsGreaterThan = <T>(d: OrdDict<T>, bound: T) => Schema.makeIsGreaterThan({ order: orderOf(d) })(bound);
export const flickMakeIsGreaterThanOrEqualTo = <T>(d: OrdDict<T>, bound: T) =>
  Schema.makeIsGreaterThanOrEqualTo({ order: orderOf(d) })(bound);
export const flickMakeIsLessThan = <T>(d: OrdDict<T>, bound: T) => Schema.makeIsLessThan({ order: orderOf(d) })(bound);
export const flickMakeIsLessThanOrEqualTo = <T>(d: OrdDict<T>, bound: T) =>
  Schema.makeIsLessThanOrEqualTo({ order: orderOf(d) })(bound);
export const flickMakeIsBetween = <T>(d: OrdDict<T>, minimum: T, maximum: T) =>
  Schema.makeIsBetween({ order: orderOf(d) })({ minimum, maximum });
export const flickMakeIsMultipleOf = <T>(remainder: (input: T, divisor: T) => T, zero: T, divisor: T) =>
  Schema.makeIsMultipleOf({ remainder, zero })(divisor);

/** `Check.apply check codec`. */
export const flickCheck = <T>(check: SchemaAST.Check<T>, codec: Of<T>): AnySchema => codec.check(check);

// Codecs.

const lowered = (t: SchemaTransformation.Transformation<string, string>): AnySchema =>
  Schema.String.pipe(Schema.decodeTo(Schema.String, t));

export const flickToLowerCase = lowered(SchemaTransformation.toLowerCase());
export const flickToUpperCase = lowered(SchemaTransformation.toUpperCase());
export const flickCapitalize = lowered(SchemaTransformation.capitalize());
export const flickUncapitalize = lowered(SchemaTransformation.uncapitalize());
export const flickSnakeToCamel = lowered(SchemaTransformation.snakeToCamel());

export const flickFiniteFromString = fallible(Schema.FiniteFromString);
export const flickStringFromBase64 = fallible(Schema.StringFromBase64);
export const flickStringFromBase64Url = fallible(Schema.StringFromBase64Url);
export const flickStringFromHex = fallible(Schema.StringFromHex);
export const flickStringFromUriComponent = fallible(Schema.StringFromUriComponent);

/** Flick's bytes: a list of whole numbers from 0 to 255. */
const byteList = Schema.Array(Schema.Number.check(Schema.isInt(), Schema.isBetween({ minimum: 0, maximum: 255 })));

/** `Codec.bytes_*`: a codec of a `Uint8Array` with the bytes as a Flick list. */
const asBytes = (codec: Schema.Codec<Uint8Array, string>): AnySchema =>
  fallible(
    codec.pipe(
      Schema.decodeTo(
        byteList,
        SchemaTransformation.transform({
          decode: (u: Uint8Array): ReadonlyArray<number> => Array.from(u),
          encode: (xs: ReadonlyArray<number>): Uint8Array => Uint8Array.from(xs),
        }),
      ),
    ),
  );

export const flickBytesFromBase64 = asBytes(Schema.Uint8ArrayFromBase64);
export const flickBytesFromBase64Url = asBytes(Schema.Uint8ArrayFromBase64Url);
export const flickBytesFromHex = asBytes(Schema.Uint8ArrayFromHex);

/** `Codec.json_string codec`: JSON text in a string, decoded by `codec`. */
export const flickFromJsonString = (codec: AnySchema): AnySchema => fallible(Schema.fromJsonString(codec));

/** `Codec.split separator`: a string as the list of its parts. */
export const flickSplit = (separator: string): AnySchema =>
  Schema.String.pipe(
    Schema.decodeTo(Schema.Array(Schema.String), {
      decode: SchemaGetter.split({ separator }),
      encode: SchemaGetter.transform((parts: ReadonlyArray<string>) => parts.join(separator)),
    }),
  );

const entryList = (value: AnySchema) => Schema.Array(Schema.Tuple([Schema.String, Schema.toType(value)]));

const asEntries = (record: Of<Readonly<Record<string, unknown>>>, value: AnySchema): AnySchema =>
  record.pipe(
    Schema.decodeTo(
      entryList(value),
      SchemaTransformation.transform({
        decode: (o: Readonly<Record<string, unknown>>): ReadonlyArray<readonly [string, unknown]> => Object.entries(o),
        encode: (entries: ReadonlyArray<readonly [string, unknown]>): Readonly<Record<string, unknown>> =>
          Object.fromEntries(entries),
      }),
    ),
  );

/** The record of a dictionary codec and the checks of its keys, for adding more. */
const dictionaries = new WeakMap<AnySchema, { readonly value: AnySchema; readonly checks: ReadonlyArray<SchemaAST.Check<object>> }>();

const dictionary = (value: AnySchema, checks: ReadonlyArray<SchemaAST.Check<object>>): AnySchema => {
  const [first, ...rest] = checks;
  const record = Schema.Record(Schema.String, value);
  const codec = asEntries(first ? record.check(first, ...rest) : record, value);
  dictionaries.set(codec, { value, checks });
  return codec;
};

/** `Codec.entries value`: a JSON object as the list of its entries. */
export const flickEntries = (value: AnySchema): AnySchema => dictionary(value, []);

/** A check of the object of an `entries` codec; another codec has no object to check. */
const entriesCheck = (check: SchemaAST.Check<object>, codec: AnySchema): AnySchema => {
  const dict = dictionaries.get(codec);
  if (dict === undefined) throw new TypeError("a property check applies to a codec made by `Codec.entries`");
  return dictionary(dict.value, [...dict.checks, check]);
};

export const flickIsMinProperties = (n: number, codec: AnySchema) => entriesCheck(Schema.isMinProperties(n), codec);
export const flickIsMaxProperties = (n: number, codec: AnySchema) => entriesCheck(Schema.isMaxProperties(n), codec);
export const flickIsBetweenProperties = (min: number, max: number, codec: AnySchema) =>
  entriesCheck(Schema.isBetweenProperties(min, max), codec);
/** Every key of the object decodes with `key`. */
export const flickIsPropertyNames = (key: AnySchema, codec: AnySchema) => entriesCheck(Schema.isPropertyNames(key), codec);

/** `Codec.split_key_value separator key_value_separator`: `a=1,b=2` as its entries. */
export const flickSplitKeyValue = (separator: string, keyValueSeparator: string): AnySchema =>
  fallible(
    asEntries(
      Schema.String.pipe(
        Schema.decodeTo(
          Schema.Record(Schema.String, Schema.String),
          SchemaTransformation.splitKeyValue({ separator, keyValueSeparator }),
        ),
      ),
      Schema.String,
    ),
  );

/** `Codec.Annotations`. */
interface Annotations {
  readonly title: Option.Option<string>;
  readonly description: Option.Option<string>;
}

export const flickAnnotate = (annotations: Annotations, codec: AnySchema): AnySchema =>
  codec.annotate({
    title: Option.getOrUndefined(annotations.title),
    description: Option.getOrUndefined(annotations.description),
  });

export const flickResolveAnnotations = (codec: AnySchema): Annotations => {
  const notes = Schema.resolveAnnotations(codec);
  return Object.freeze({
    title: Option.fromUndefinedOr(notes?.title),
    description: Option.fromUndefinedOr(notes?.description),
  });
};

/** `Codec.to_schema codec`: the decoded side, whose canonical JSON form is `codec`. */
export const flickToType = (codec: AnySchema): AnySchema => withJson(Schema.toType(codec), () => codec);

/** `Codec.wire_schema codec`: the wire side, written as itself. */
export const flickToEncoded = (codec: AnySchema): AnySchema => {
  const wire = Schema.toEncoded(codec);
  return withJson(wire, () => wire);
};

const effectError = (issue: SchemaIssue.Issue): SchemaError =>
  toSchemaError(false)(new Schema.SchemaError(issue));

const recovered = <A>(r: Result.Result<A, SchemaError>) =>
  Result.isSuccess(r) ? Effect.succeed(Option.some(r.success)) : Effect.fail(issueOf(r.failure));

/** A fallback accepts input the wire schema does not describe. */
const REFUSE_FALLBACK = { "flick/refuse": "a decoding fallback accepts input the wire schema does not state" } satisfies Notes;

export const flickCatchDecoding = (f: (error: SchemaError) => Result.Result<unknown, SchemaError>, codec: AnySchema): AnySchema =>
  noted(
    Schema.catchDecoding((issue) => recovered(f(effectError(issue))))(codec),
    REFUSE_FALLBACK,
  );

export const flickCatchDecodingWithContext = (
  f: (error: SchemaError) => Effect.Effect<unknown, SchemaError, unknown>,
  codec: AnySchema,
): AnySchema =>
  noted(
    Schema.catchDecodingWithContext((issue) => Effect.mapError(Effect.map(f(effectError(issue)), Option.some), issueOf))(
      codec,
    ),
    { ...REFUSE_FALLBACK, "flick/transform": "effect" },
  );

export const flickCatchEncoding = (f: (error: SchemaError) => Result.Result<unknown, SchemaError>, codec: AnySchema): AnySchema =>
  Schema.catchEncoding((issue) => recovered(f(toSchemaError(true)(new Schema.SchemaError(issue)))))(codec);

export const flickCatchEncodingWithContext = (
  f: (error: SchemaError) => Effect.Effect<unknown, SchemaError, unknown>,
  codec: AnySchema,
): AnySchema =>
  noted(
    Schema.catchEncodingWithContext((issue) =>
      Effect.mapError(Effect.map(f(toSchemaError(true)(new Schema.SchemaError(issue))), Option.some), issueOf),
    )(codec),
    { "flick/effect": true },
  );

/** `Codec.check_effect f codec`: a runtime check of the decoded value. */
export const flickCheckEffect = (f: (value: unknown) => Effect.Effect<boolean, SchemaError, unknown>, codec: AnySchema): AnySchema =>
  noted(
    codec.pipe(
      Schema.decodeTo(Schema.toType(codec), {
      decode: SchemaGetter.checkEffect((value: unknown) =>
        Effect.map(Effect.result(f(value)), (r) =>
          Result.isSuccess(r) ? r.success : r.failure._0.map((i) => i.message).join("; "),
        ),
      ),
        encode: SchemaGetter.passthrough(),
      }),
    ),
    { "flick/transform": "effect" },
  );

/** `Codec.encode_xml codec value`. */
export const flickToEncoderXml = (codec: AnySchema, value: unknown): Effect.Effect<string, SchemaError, unknown> =>
  Effect.mapError(Schema.toEncoderXml(codec)(value), (issue) => toSchemaError(true)(new Schema.SchemaError(issue)));

// Schemas and codecs of the standard library's types.

const nonNegative = Schema.makeFilter((d: Duration.Duration) => Duration.isGreaterThanOrEqualTo(d, Duration.zero), {
  expected: "a non-negative duration",
});

/** `Duration.json_millis`: a finite, non-negative number of milliseconds. */
export const flickDurationFromMillis: AnySchema = jsonNumber
  .check(Schema.isGreaterThanOrEqualTo(0))
  .pipe(Schema.decodeTo(Schema.Duration, SchemaTransformation.durationFromMillis));

/** `Duration.json_nanos`: a non-negative number of nanoseconds as a string of digits. */
export const flickDurationFromNanos: AnySchema = Schema.String.check(Schema.isPattern(/^[0-9]+$/)).pipe(
  Schema.decodeTo(Schema.BigInt, SchemaTransformation.bigintFromString),
  Schema.decodeTo(Schema.Duration, SchemaTransformation.durationFromNanos),
);

/** `Duration.json_string`: Effect's text form, `"1500 millis"`. */
export const flickDurationFromString: AnySchema = fallible(Schema.DurationFromString.check(nonNegative));

export const flickDuration: AnySchema = withJson(Schema.Duration.check(nonNegative), () => flickDurationFromMillis);

/** `BigDecimal.json`: a decimal as its string. */
export const flickBigDecimalFromString: AnySchema = fallible(Schema.BigDecimalFromString);

export const flickBigDecimal: AnySchema = withJson(Schema.BigDecimal, () => flickBigDecimalFromString);

const chunkOf = (item: AnySchema): AnySchema =>
  Schema.Array(item).pipe(
    Schema.decodeTo(
      Schema.Chunk(Schema.toType(item)),
      SchemaTransformation.transform({
        decode: (xs: ReadonlyArray<unknown>) => Chunk.fromIterable(xs),
        encode: (c: Chunk.Chunk<unknown>) => Chunk.toReadonlyArray(c),
      }),
    ),
  );

/** `Chunk.json item`: a chunk as a JSON array. */
export const flickChunkJson = (item: AnySchema): AnySchema => chunkOf(item);

export const flickChunk = (item: AnySchema): AnySchema => withJson(Schema.Chunk(item), () => chunkOf(jsonOf(item)));

const isFlickSet = (u: unknown): u is FlickHashSet<unknown> =>
  typeof u === "object" && u !== null && "native" in u && HashSet.isHashSet(u.native);

/** `HashSet.json item`: a set as a JSON array of its elements, rebuilt with the canonical witness. */
export const flickHashSetJson = (d: HashDict<unknown>, item: AnySchema): AnySchema =>
  Schema.Array(item).pipe(
    Schema.decodeTo(
      Schema.declare(isFlickSet, { expected: "a HashSet" }),
      SchemaTransformation.transform({
        decode: (xs: ReadonlyArray<unknown>) => hashSetFromList(eqHashCanonical(d), xs),
        encode: (s: FlickHashSet<unknown>): ReadonlyArray<unknown> => hashSetToList(s),
      }),
    ),
  );

const isFlickMap = (u: unknown): u is FlickHashMap<unknown, unknown> =>
  typeof u === "object" && u !== null && "native" in u && HashMap.isHashMap(u.native);

/** `HashMap.json key value`: a map as a JSON array of `[key, value]` pairs. */
export const flickHashMapJson = (d: HashDict<unknown>, key: AnySchema, value: AnySchema): AnySchema =>
  Schema.Array(Schema.Tuple([key, value])).pipe(
    Schema.decodeTo(
      Schema.declare(isFlickMap, { expected: "a HashMap" }),
      SchemaTransformation.transform({
        decode: (entries: ReadonlyArray<readonly [unknown, unknown]>) =>
          entries.reduce((m, [k, v]) => hashMapInsert(k, v, m), hashMapWith<unknown, unknown>(eqHashCanonical(d))),
        encode: (m: FlickHashMap<unknown, unknown>): ReadonlyArray<readonly [unknown, unknown]> => hashMapToList(m),
      }),
    ),
  );
