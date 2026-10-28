// Wire codecs: Effect Schema codecs from a JSON-like wire form to values of
// the program. A `codec` block is a struct of its field codecs whose keys
// are the wire names; extra keys are refused unless it ignores them.
import { Effect, Option, Redacted, Result, Schema, SchemaGetter, SchemaIssue, SchemaTransformation } from "effect";

import { type SchemaError, isSchemaError } from "./issues.ts";
import { type AnySchema, anyOption, anyRedacted, jsonBigint, jsonInt, jsonNumber, jsonOf } from "./schemas.ts";

export const codecString: AnySchema = Schema.String;
export const codecNumberFinite: AnySchema = jsonNumber;
export const codecInt: AnySchema = jsonInt;
export const codecBool: AnySchema = Schema.Boolean;
export const codecBigint: AnySchema = jsonBigint;

/** The canonical JSON codec of a schema. */
export const codecJson = (schema: AnySchema): AnySchema => jsonOf(schema);

const LOSSLESS = ["NaN", "Infinity", "-Infinity", "-0"] as const;

/** `NaN`, the infinities and `-0` as strings, every other number as itself. */
export const codecNumberLossless: AnySchema = /*#__PURE__*/ Schema.Union([Schema.Number, Schema.Literals(LOSSLESS)])
  .annotate({ expected: "a number" })
  .pipe(
    Schema.decodeTo(
      Schema.Number,
      SchemaTransformation.transform({
        decode: (v: number | (typeof LOSSLESS)[number]) => (typeof v === "number" ? v : v === "-0" ? -0 : Number(v)),
        encode: (n: number): number | (typeof LOSSLESS)[number] => {
          if (Object.is(n, -0)) return "-0";
          if (Number.isFinite(n)) return n;
          return Number.isNaN(n) ? "NaN" : n > 0 ? "Infinity" : "-Infinity";
        },
      }),
    ),
  );

export const codecOptionNull = (item: AnySchema): AnySchema => Schema.OptionFromNullOr(item);

/** `null`, `undefined` and a missing key are `None`; `None` is written `null`. */
export const codecOptionNullish = (item: AnySchema): AnySchema =>
  Schema.optional(Schema.NullOr(item)).pipe(
    Schema.decodeTo(anyOption, {
      decode: SchemaGetter.transformOptional((o: Option.Option<unknown>) =>
        Option.some(Option.flatMap(o, Option.fromNullishOr)),
      ),
      encode: SchemaGetter.transformOptional((o: Option.Option<Option.Option<unknown>>) =>
        Option.some(Option.getOrNull(Option.flatten(o))),
      ),
    }),
  );

type TaggedWire = { readonly _tag: "None" } | { readonly _tag: "Some"; readonly value: unknown };

export const codecTaggedOption = (item: AnySchema): AnySchema =>
  Schema.Union([Schema.Struct({ _tag: Schema.Literal("None") }), Schema.Struct({ _tag: Schema.Literal("Some"), value: item })])
    .annotate({ expected: "an object with `_tag` None or Some", "flick/tags": ["None", "Some"], "flick/oneOf": true })
    .pipe(
      Schema.decodeTo(
        anyOption,
        SchemaTransformation.transform({
          decode: (v: TaggedWire) => (v._tag === "None" ? Option.none() : Option.some(v.value)),
          encode: (o: Option.Option<unknown>): TaggedWire =>
            Option.isSome(o) ? { _tag: "Some", value: o.value } : { _tag: "None" },
        }),
      ),
    );

export const codecList = (item: AnySchema): AnySchema => Schema.Array(item);

/** The issue of a `SchemaError` of the program. */
export const issueOf = (error: unknown): SchemaIssue.Issue =>
  new SchemaIssue.InvalidValue({
    message: isSchemaError(error) ? error._0.map((i) => i.message).join("; ") : String(error),
  });

/** `from` decoded further by `decode`, which `flick/transform` names for the exporter. */
const transformed = (
  mode: "pure" | "result" | "effect",
  decode: SchemaGetter.Getter<unknown, unknown, unknown>,
  encode: (value: unknown) => unknown,
  from: AnySchema,
): AnySchema =>
  from.pipe(
    Schema.decodeTo(Schema.Unknown.annotate({ "flick/transform": mode }), {
      decode,
      encode: SchemaGetter.transform(encode),
    }),
  );

/** `Codec.transform`: a pure function each way. */
export const codecTransformPure = (
  decode: (wire: unknown) => unknown,
  encode: (value: unknown) => unknown,
  from: AnySchema,
): AnySchema => transformed("pure", SchemaGetter.transform(decode), encode, from);

/** `Codec.transform_result`: decoding may fail with a `SchemaError`. */
export const codecTransformResult = (
  decode: (wire: unknown) => Result.Result<unknown, SchemaError>,
  encode: (value: unknown) => unknown,
  from: AnySchema,
): AnySchema =>
  transformed(
    "result",
    SchemaGetter.transformEffect((v: unknown) => {
      const r = decode(v);
      return Result.isSuccess(r) ? Effect.succeed(r.success) : Effect.fail(issueOf(r.failure));
    }),
    encode,
    from,
  );

/** `Codec.transform_effect`: decoding is a runtime task. */
export const codecTransformEffect = (
  decode: (wire: unknown) => Effect.Effect<unknown, SchemaError, unknown>,
  encode: (value: unknown) => unknown,
  from: AnySchema,
): AnySchema =>
  transformed(
    "effect",
    SchemaGetter.transformEffect((v: unknown) => Effect.mapError(decode(v), issueOf)),
    encode,
    from,
  );

/** A whole-value check after decoding. */
export const codecCheck = (check: (value: unknown) => boolean, message: string, from: AnySchema): AnySchema =>
  from.check(Schema.makeFilter((v: unknown) => check(v), { message }));

/** A Redacted value as its content's wire form: an explicit opt-in. */
export const codecRedactedReveal = (item: AnySchema): AnySchema =>
  item.pipe(
    Schema.decodeTo(
      anyRedacted,
      SchemaTransformation.transform({
        decode: (v: unknown) => Redacted.make(v),
        encode: (r: Redacted.Redacted<unknown>) => Redacted.value(r),
      }),
    ),
  );

/** A Redacted value written as `"<redacted>"`; it cannot be decoded. */
export const codecRedactedMasked: AnySchema = /*#__PURE__*/ Schema.Literal("<redacted>").pipe(
  Schema.decodeTo(anyRedacted, {
    decode: SchemaGetter.forbidden(() => "a masked Redacted value cannot be decoded"),
    encode: SchemaGetter.transform((): "<redacted>" => "<redacted>"),
  }),
);

/** A field of a `codec` block, before its label is known. */
export interface FieldSpec {
  readonly wire: string;
  readonly codec: AnySchema;
  readonly optional: boolean;
  readonly onMissing?: (unit: undefined) => unknown;
  readonly onMissingEffect?: (unit: undefined) => Effect.Effect<unknown, SchemaError, unknown>;
}

/** A field of a `codec` block. */
export interface Field extends FieldSpec {
  readonly label: string;
}

/** The Effect SchemaError of a `SchemaError` of the program. */
const effectError = (error: SchemaError): Schema.SchemaError => new Schema.SchemaError(issueOf(error));

/** The codec of one field of a block. */
const fieldCodec = (f: Field): AnySchema => {
  if (f.optional) return Schema.OptionFromOptionalKey(f.codec);
  const { onMissing, onMissingEffect } = f;
  if (onMissing) {
    return f.codec.pipe(Schema.withDecodingDefaultTypeKey(Effect.sync(() => onMissing(undefined))));
  }
  if (onMissingEffect) {
    return f.codec.pipe(
      Schema.withDecodingDefaultTypeKey(Effect.mapError(Effect.suspend(() => onMissingEffect(undefined)), effectError)),
    );
  }
  return f.codec;
};

/** What a block was made of, for `ignore_extra_fields`. */
const blocks = /*#__PURE__*/ new WeakMap<AnySchema, { readonly name: string; readonly fields: ReadonlyArray<Field> }>();

/** A record codec of the `codec` DSL. */
export const codecFields = (name: string, fields: ReadonlyArray<Field>, ignoreExtra: boolean): AnySchema => {
  const wireNames = Object.fromEntries(fields.map((f) => [f.label, f.wire]));
  const struct = Schema.Struct(Object.fromEntries(fields.map((f) => [f.label, fieldCodec(f)]))).annotate({
    expected: "an object",
    "flick/block": name,
    "flick/wires": wireNames,
  });
  const renamed: AnySchema = fields.some((f) => f.label !== f.wire) ? struct.pipe(Schema.encodeKeys(wireNames)) : struct;
  const wires = new Set(fields.map((f) => f.wire));
  const codec: AnySchema = ignoreExtra
    ? Schema.Record(Schema.String, Schema.Unknown)
        .annotate({ expected: "an object", "flick/ignoreExtra": true })
        .pipe(
          Schema.decodeTo(renamed, {
            decode: SchemaGetter.transform((o: Readonly<Record<string, unknown>>) =>
              Object.fromEntries(Object.entries(o).filter(([k]) => wires.has(k))),
            ),
            // what a block encodes is already the object of its wire names
            encode: SchemaGetter.passthrough({ strict: false }),
          }),
        )
    : renamed;
  blocks.set(codec, { name, fields });
  return codec;
};

export const codecIgnoreExtra = (codec: AnySchema): AnySchema => {
  const block = blocks.get(codec);
  return block ? codecFields(block.name, block.fields, true) : codec;
};
