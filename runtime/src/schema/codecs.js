// Wire codecs: Effect Schema codecs from a JSON-like wire form to values of
// the program. A `codec` block is a struct of its field codecs whose keys
// are the wire names; extra keys are refused unless it ignores them.
import { Effect, Option, Redacted, Schema, SchemaGetter, SchemaIssue, SchemaTransformation } from "effect";

import { anyOption, anyRedacted, jsonBigint, jsonNumber, jsonOf } from "./schemas.js";

export const codecString = Schema.String;
export const codecNumberFinite = jsonNumber;
export const codecBool = Schema.Boolean;
export const codecBigint = jsonBigint;

/** The canonical JSON codec of a schema. */
export const codecJson = (schema) => jsonOf(schema);

/** `NaN`, the infinities and `-0` as strings, every other number as itself. */
export const codecNumberLossless = Schema.Union([
  Schema.Number,
  Schema.Literals(["NaN", "Infinity", "-Infinity", "-0"]),
])
  .annotate({ expected: "a number" })
  .pipe(
    Schema.decodeTo(
      Schema.Number,
      SchemaTransformation.transform({
        decode: (v) => (typeof v === "number" ? v : v === "-0" ? -0 : Number(v)),
        encode: (n) => (Object.is(n, -0) ? "-0" : Number.isFinite(n) ? n : String(n)),
      }),
    ),
  );

export const codecOptionNull = (item) => Schema.OptionFromNullOr(item);

/** `null`, `undefined` and a missing key are `None`; `None` is written `null`. */
export const codecOptionNullish = (item) =>
  Schema.optional(Schema.NullOr(item)).pipe(
    Schema.decodeTo(anyOption, {
      decode: SchemaGetter.transformOptional((o) => Option.some(Option.flatMap(o, Option.fromNullishOr))),
      encode: SchemaGetter.transformOptional((o) => Option.some(Option.getOrNull(Option.flatten(o)))),
    }),
  );

export const codecTaggedOption = (item) =>
  Schema.Union([Schema.Struct({ _tag: Schema.Literal("None") }), Schema.Struct({ _tag: Schema.Literal("Some"), value: item })])
    .annotate({ expected: "an object with `_tag` None or Some", "flick/tags": ["None", "Some"], "flick/oneOf": true })
    .pipe(
      Schema.decodeTo(
        anyOption,
        SchemaTransformation.transform({
          decode: (v) => (v._tag === "None" ? Option.none() : Option.some(v.value)),
          encode: (o) => (Option.isSome(o) ? { _tag: "Some", value: o.value } : { _tag: "None" }),
        }),
      ),
    );

export const codecList = (item) => Schema.Array(item);

/** The issue of a `SchemaError` of the program. */
const issueOf = (error) =>
  new SchemaIssue.InvalidValue({ message: error?._0?.map((i) => i.message).join("; ") ?? String(error) });

/** `mode` is `pure`, `result` (decode may fail) or `effect` (decode is runtime). */
export const codecTransform = (mode, decode, encode, from) => {
  const getter = {
    pure: () => SchemaGetter.transform(decode),
    result: () =>
      SchemaGetter.transformEffect((v) => {
        const r = decode(v);
        return r._tag === "Success" ? Effect.succeed(r.success) : Effect.fail(issueOf(r.failure));
      }),
    effect: () => SchemaGetter.transformEffect((v) => Effect.mapError(decode(v), issueOf)),
  }[mode]();
  return from.pipe(
    Schema.decodeTo(Schema.Unknown.annotate({ "flick/transform": mode }), {
      decode: getter,
      encode: SchemaGetter.transform(encode),
    }),
  );
};

/** A whole-value check after decoding. */
export const codecCheck = (check, message, from) => from.check(Schema.makeFilter((v) => check(v), { message }));

/** A Redacted value as its content's wire form: an explicit opt-in. */
export const codecRedactedReveal = (item) =>
  item.pipe(Schema.decodeTo(anyRedacted, SchemaTransformation.transform({ decode: Redacted.make, encode: Redacted.value })));

/** A Redacted value written as `"<redacted>"`; it cannot be decoded. */
export const codecRedactedMasked = Schema.Literal("<redacted>").pipe(
  Schema.decodeTo(anyRedacted, {
    decode: SchemaGetter.forbidden(() => "a masked Redacted value cannot be decoded"),
    encode: SchemaGetter.transform(() => "<redacted>"),
  }),
);

/** The Effect SchemaError of a `SchemaError` of the program. */
const effectError = (error) => new Schema.SchemaError(issueOf(error));

/** The codec of one field of a block. */
const fieldCodec = (f) => {
  if (f.optional) return Schema.OptionFromOptionalKey(f.codec);
  if (f.onMissing) {
    return f.codec.pipe(Schema.withDecodingDefaultTypeKey(Effect.sync(() => f.onMissing(undefined))));
  }
  if (f.onMissingEffect) {
    return f.codec.pipe(
      Schema.withDecodingDefaultTypeKey(Effect.mapError(Effect.suspend(() => f.onMissingEffect(undefined)), effectError)),
    );
  }
  return f.codec;
};

/** What a block was made of, for `ignore_extra_fields`. */
const blocks = new WeakMap();

/**
 * A record codec of the `codec` DSL: `fields` lists
 * `{ label, wire, codec, optional, onMissing, onMissingEffect }`.
 */
export const codecFields = (name, fields, ignoreExtra) => {
  const wireNames = Object.fromEntries(fields.map((f) => [f.label, f.wire]));
  const struct = Schema.Struct(Object.fromEntries(fields.map((f) => [f.label, fieldCodec(f)]))).annotate({
    expected: "an object",
    "flick/block": name,
    "flick/wires": wireNames,
  });
  const renamed = fields.some((f) => f.label !== f.wire) ? struct.pipe(Schema.encodeKeys(wireNames)) : struct;
  const wires = new Set(fields.map((f) => f.wire));
  const codec = ignoreExtra
    ? Schema.Record(Schema.String, Schema.Unknown)
        .annotate({ expected: "an object", "flick/ignoreExtra": true })
        .pipe(
          Schema.decodeTo(renamed, {
            decode: SchemaGetter.transform((o) => Object.fromEntries(Object.entries(o).filter(([k]) => wires.has(k)))),
            encode: SchemaGetter.transform((o) => o),
          }),
        )
    : renamed;
  blocks.set(codec, { name, fields });
  return codec;
};

export const codecIgnoreExtra = (codec) => {
  const block = blocks.get(codec);
  return block ? codecFields(block.name, block.fields, true) : codec;
};
