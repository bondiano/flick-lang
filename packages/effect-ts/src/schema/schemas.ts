// A schema of the program is an Effect Schema of the runtime values of a
// type. `jsonOf` gives each the codec of its canonical JSON form, built out
// of field and element codecs, so decoding a tree walks it once. Notes for
// the exporter, the formatter of issues and the generators are annotations
// under `flick/…` (see notes.ts).
import { Option, Redacted, Result, Schema, SchemaGetter, SchemaTransformation } from "effect";

import { erase } from "@flick-lang/runtime/internal";
import type { Rng } from "../stdlib/testing.ts";
import type { Notes } from "./notes.ts";

/**
 * A schema or codec of the program. Its value types are the compiler's to
 * know: generated code passes witnesses whose types it checked.
 */
export type AnySchema = Schema.Top;

/** `schema` with the runtime's annotations, which Effect keeps as they are. */
export const annotate = (schema: AnySchema, notes: Notes): AnySchema => schema.annotate(notes);

/** How to build the JSON codec of a schema, by schema. */
const jsonMakers = new WeakMap<AnySchema, () => AnySchema>();
const jsonCodecs = /*#__PURE__*/ new WeakMap<AnySchema, AnySchema>();

export type FlnDesc =
  | { readonly kind: "number" | "string" | "bool" | "int" | "bigint" | "unit" | "never" | "unknown" | "int8" | "int16" | "int64" | "uint8" | "uint16" | "uint32" | "uint64" | "float32" }
  | { readonly kind: "list" | "option"; readonly item: AnySchema }
  | { readonly kind: "result"; readonly error: AnySchema; readonly value: AnySchema }
  | { readonly kind: "tuple"; readonly items: ReadonlyArray<AnySchema> }
  | { readonly kind: "record"; readonly fields: ReadonlyArray<readonly [string, AnySchema]> }
  | { readonly kind: "adt"; readonly variants: ReadonlyArray<Variant> }
  | { readonly kind: "lazy"; readonly get: () => AnySchema }
  | { readonly kind: "opaque"; readonly reason: string }
  | { readonly kind: "literals"; readonly values: ReadonlyArray<string | number> }
  | { readonly kind: "enum-tags"; readonly pairs: ReadonlyArray<readonly [string, string | number]> }
  | { readonly kind: "refined"; readonly name: string; readonly base: AnySchema; readonly check: (value: unknown) => boolean };

const flnDescs = /*#__PURE__*/ new WeakMap<AnySchema, FlnDesc>();
export const flnDescOf = (schema: AnySchema): FlnDesc | undefined => flnDescs.get(schema);

export const withJson = <S extends AnySchema>(schema: S, make: () => AnySchema, fln?: FlnDesc): S => {
  jsonMakers.set(schema, make);
  if (fln !== undefined) flnDescs.set(schema, fln);
  return schema;
};

/** A codec whose wire form `reason` explains is missing, both ways. */
export const refuse = (reason: string): AnySchema =>
  annotate(Schema.Unknown, { "flick/refuse": reason }).pipe(
    Schema.decodeTo(Schema.Unknown, {
      decode: SchemaGetter.forbidden(() => reason),
      encode: SchemaGetter.forbidden(() => reason),
    }),
  );

/** The codec of the canonical JSON form of `schema`, made once. */
export const jsonOf = (schema: AnySchema): AnySchema => {
  let codec = jsonCodecs.get(schema);
  if (codec === undefined) {
    const make = jsonMakers.get(schema);
    codec = make ? make() : refuse("a schema made outside Flick has no canonical JSON form");
    jsonCodecs.set(schema, codec);
  }
  return codec;
};

/** `from` mapped both ways onto `to`, a shallow check of the decoded side. */
const mapped =
  <T, E>(to: Schema.Codec<T>, decode: (e: E) => T, encode: (t: T) => E) =>
  (from: Schema.Codec<E, unknown, unknown, unknown>): AnySchema =>
    from.pipe(Schema.decodeTo(to, SchemaTransformation.transform({ decode, encode })));

export const anyOption: Schema.Codec<Option.Option<unknown>> = /*#__PURE__*/ Schema.declare(Option.isOption, { expected: "an Option" });
export const anyResult: Schema.Codec<Result.Result<unknown, unknown>> = /*#__PURE__*/ Schema.declare(Result.isResult, {
  expected: "a Result",
});
export const anyRedacted: Schema.Codec<Redacted.Redacted<unknown>> = /*#__PURE__*/ Schema.declare(Redacted.isRedacted, {
  expected: "a Redacted value",
});

const finite = /*#__PURE__*/ Schema.makeFilter((n: number) => Number.isFinite(n), {
  expected: "a finite number",
  "flick/wire": true,
} satisfies Notes);

/** A JSON number: finite both ways. */
export const jsonNumber: Schema.Codec<number> = /*#__PURE__*/ Schema.Number.annotate({ expected: "a finite number" }).check(finite);

const int32 = /*#__PURE__*/ Schema.makeFilter((n: number) => (n | 0) === n, {
  message: "expected an Int, an integer from -2147483648 to 2147483647",
  "flick/known": "int",
} satisfies Notes);

export const jsonInt: Schema.Codec<number> = /*#__PURE__*/ Schema.Number.annotate({ expected: "an Int" }).check(int32);

const digits = /*#__PURE__*/ Schema.makeFilter((s: string) => /^-?\d+$/.test(s), {
  message: "expected a string of digits",
  "flick/pattern": "^-?[0-9]+$",
} satisfies Notes);

/** A BigInt as a string of digits. */
export const jsonBigint: AnySchema = /*#__PURE__*/ mapped(
  Schema.BigInt,
  (s: string) => BigInt(s),
  (n) => String(n),
)(Schema.String.annotate({ expected: "a string of digits" }).check(digits));

const jsonUnit = /*#__PURE__*/ mapped(
  Schema.Undefined,
  () => undefined,
  () => null,
)(Schema.Null);

const intGen = (span: number, offset: number) => (rng: () => number) => Math.floor(rng() * span) - offset;
const bigintGen = (span: number, offset: number) => (rng: () => number) => BigInt(Math.floor(rng() * span) - offset);

const boundedIntSchema = (
  kind: "int8" | "int16" | "uint8" | "uint16" | "uint32",
  label: string,
  min: number,
  max: number,
  genSpan: number,
  genOffset: number,
): AnySchema => {
  const message = `expected ${label}, an integer from ${min} to ${max}`;
  const checked = Schema.Number.check(
    Schema.isInt({ message }),
    Schema.isBetween({ minimum: min, maximum: max }, { message }),
  );
  const type = annotate(checked, { expected: label, "flick/generator": intGen(genSpan, genOffset) });
  return withJson(type, () => checked, { kind });
};

const boundedBigIntSchema = (
  kind: "int64" | "uint64",
  label: string,
  min: bigint,
  max: bigint,
  genOffset: number,
): AnySchema => {
  const check = Schema.isBetweenBigInt(
    { minimum: min, maximum: max },
    {
      message: `expected ${label}, an integer from ${String(min)} to ${String(max)}`,
      "flick/wire": true,
    } satisfies Notes,
  );
  const base = Schema.BigInt.check(check);
  const type = annotate(base, { expected: label, "flick/generator": bigintGen(2000, genOffset) });
  const json = mapped(
    base,
    (s: string) => BigInt(s),
    (n) => String(n),
  )(Schema.String.annotate({ expected: "a string of digits" }).check(digits));
  return withJson(type, () => json, { kind });
};

export const schemaInt: AnySchema = /*#__PURE__*/ withJson(Schema.Number.check(int32), () => jsonInt, { kind: "int" });
export const schemaInt8: AnySchema = /*#__PURE__*/ boundedIntSchema("int8", "an Int8", -128, 127, 256, 128);
export const schemaInt16: AnySchema = /*#__PURE__*/ boundedIntSchema("int16", "an Int16", -32768, 32767, 65536, 32768);
export const schemaInt64: AnySchema = /*#__PURE__*/ boundedBigIntSchema("int64", "an Int64", -9223372036854775808n, 9223372036854775807n, 1000);
export const schemaUInt8: AnySchema = /*#__PURE__*/ boundedIntSchema("uint8", "a UInt8", 0, 255, 256, 0);
export const schemaUInt16: AnySchema = /*#__PURE__*/ boundedIntSchema("uint16", "a UInt16", 0, 65535, 65536, 0);
export const schemaUInt32: AnySchema = /*#__PURE__*/ boundedIntSchema("uint32", "a UInt32", 0, 4294967295, 2000, 0);
export const schemaUInt64: AnySchema = /*#__PURE__*/ boundedBigIntSchema("uint64", "a UInt64", 0n, 18446744073709551615n, 0);
export const schemaFloat32: AnySchema = /*#__PURE__*/ withJson(
  annotate(Schema.Number, {
    expected: "a Float32",
    "flick/generator": (rng: () => number) => Math.fround(Math.floor(rng() * 2000) - 1000),
  }),
  () => jsonNumber,
  { kind: "float32" },
);
export const schemaNumber: AnySchema = /*#__PURE__*/ withJson(Schema.Number, () => jsonNumber, { kind: "number" });
export const schemaString: AnySchema = /*#__PURE__*/ withJson(Schema.String, () => Schema.String, { kind: "string" });
export const schemaBool: AnySchema = /*#__PURE__*/ withJson(Schema.Boolean, () => Schema.Boolean, { kind: "bool" });
export const schemaBigint: AnySchema = /*#__PURE__*/ withJson(Schema.BigInt, () => jsonBigint, { kind: "bigint" });
export const schemaUnit: AnySchema = /*#__PURE__*/ withJson(annotate(Schema.Undefined, { expected: "()" }), () => jsonUnit, { kind: "unit" });
export const schemaNever: AnySchema = /*#__PURE__*/ withJson(Schema.Never, () => Schema.Never, { kind: "never" });
export const schemaUnknown: AnySchema = /*#__PURE__*/ withJson(Schema.Unknown, () => Schema.Unknown, { kind: "unknown" });

export const schemaList = (item: AnySchema): AnySchema =>
  withJson(annotate(Schema.Array(item), { expected: "a list" }), () => Schema.Array(jsonOf(item)), { kind: "list", item });

export const schemaTuple = (items: ReadonlyArray<AnySchema>): AnySchema =>
  withJson(annotate(Schema.Tuple(items), { expected: `a tuple of ${items.length}` }), () =>
    Schema.Tuple(items.map(jsonOf)), { kind: "tuple", items },
  );

export const schemaOption = (item: AnySchema): AnySchema =>
  withJson(Schema.Option(item), () => Schema.OptionFromNullOr(jsonOf(item)), { kind: "option", item });

interface Wire {
  readonly ok?: unknown;
  readonly err?: unknown;
}

export const schemaResult = (error: AnySchema, value: AnySchema): AnySchema =>
  withJson(Schema.Result(value, error), () =>
    mapped(
      anyResult,
      (v: Wire) => ("ok" in v ? Result.succeed(v.ok) : Result.fail(v.err)),
      (r): Wire => (Result.isSuccess(r) ? { ok: r.success } : { err: r.failure }),
    )(
      Schema.Union([Schema.Struct({ ok: jsonOf(value) }), Schema.Struct({ err: jsonOf(error) })]).annotate({
        expected: "an object with `ok` or `err`",
        "flick/oneOf": true,
      }),
    ),
    { kind: "result", error, value },
  );

export const schemaRedacted = (item: AnySchema): AnySchema =>
  withJson(Schema.Redacted(item), () => refuse("a Redacted value has no JSON form without an explicit policy"), {
    kind: "opaque",
    reason: "a Redacted value has no FLN form without an explicit policy",
  });

/** A record: `fields` lists `[label, schema]`, canonicalized to UTF-16 label order. */
export const schemaRecord = (fields: ReadonlyArray<readonly [string, AnySchema]>): AnySchema => {
  const sorted = fields.toSorted(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  return withJson(annotate(Schema.Struct(Object.fromEntries(sorted)), { expected: "a record" }), () =>
    Schema.Struct(Object.fromEntries(sorted.map(([label, s]) => [label, jsonOf(s)]))),
    { kind: "record", fields: sorted },
  );
};

const positional = (fields: ReadonlyArray<AnySchema>, each: (s: AnySchema) => AnySchema): Record<string, AnySchema> =>
  Object.fromEntries(fields.map((f, i) => [`_${i}`, each(f)]));

/** The wire `_tag` of a variant: its name, decoded to its qualified id. */
const tagCodec = (name: string, tag: string): AnySchema =>
  mapped(
    Schema.Literal(tag),
    () => tag,
    () => name,
  )(Schema.Literal(name));

/** A constructor of a variant type: its qualified id, its name and the schemas of its fields. */
export type Variant = readonly [tag: string, name: string, fields: ReadonlyArray<AnySchema>];

/** A variant type: values `{ _tag, _0, _1, … }` with qualified tags. */
export const schemaAdt = (name: string, variants: ReadonlyArray<Variant>): AnySchema => {
  const names = variants.map(([, n]) => n);
  const type = annotate(
    Schema.Union(
      variants.map(([tag, , fields]) => Schema.Struct({ _tag: Schema.Literal(tag), ...positional(fields, (f) => f) })),
    ),
    { expected: `a ${name}` },
  );
  return withJson(type, () =>
    annotate(
      Schema.Union(
        variants.map(([tag, n, fields]) =>
          annotate(Schema.Struct({ _tag: tagCodec(n, tag), ...positional(fields, jsonOf) }), { "flick/variant": n }),
        ),
      ),
      { expected: "an object with a `_tag`", "flick/tags": names, "flick/oneOf": true },
    ),
    { kind: "adt", variants },
  );
};

/** An enum: its values are its literals, and so is its wire form; with `tags`, the wire form of a variant type, `{ _tag }` of each literal's constructor. */
export const schemaEnum = (literals: ReadonlyArray<string | number>, tags: ReadonlyArray<string> | null): AnySchema => {
  const type = Schema.Union(literals.map((l) => Schema.Literal(l)));
  if (tags === null) return withJson(type, () => type, { kind: "literals", values: literals });
  return withJson(type, () =>
    annotate(
      Schema.Union(
        literals.map((l, i) => {
          const tag = tags[i] ?? "";
          return annotate(
            mapped(
              Schema.Literal(l),
              () => l,
              () => ({ _tag: tag }),
            )(Schema.Struct({ _tag: Schema.Literal(tag) })),
            { "flick/variant": tag },
          );
        }),
      ),
      { expected: "an object with a `_tag`", "flick/tags": tags, "flick/oneOf": true },
    ),
    { kind: "enum-tags", pairs: literals.map((l, i) => [tags[i] ?? "", l] as const) },
  );
};

/** A refined type: `base` and a pure predicate; `known` names a check exporters know. */
export const schemaRefined = (
  name: string,
  base: AnySchema,
  check: (value: never) => boolean,
  known: string | null,
): AnySchema => {
  const checked = erase(check);
  const filter = () =>
    Schema.makeFilter((v: unknown) => checked(v), {
      message: `not a valid ${name}`,
      "flick/known": known,
      "flick/refined": name,
    } satisfies Notes);
  return withJson(base.check(filter()), () => jsonOf(base).check(filter()), { kind: "refined", name, base, check: checked });
};

/** A reference resolved on use: recursive and mutually recursive types. */
export const schemaLazy = (name: string, get: () => AnySchema): AnySchema => {
  // a generic type's witness makes a new schema per call: resolve it once
  let target: AnySchema | undefined;
  const once = (): AnySchema => (target ??= get());
  return withJson(annotate(Schema.suspend(once), { "flick/ref": name }), () =>
    annotate(
      Schema.suspend(() => jsonOf(once())),
      { "flick/ref": name },
    ),
    { kind: "lazy", get: once },
  );
};

/** A type whose owner gives no wire form, as a scoped resource. */
export const schemaOpaque = (name: string, reason: string): AnySchema =>
  withJson(annotate(Schema.Unknown, { expected: name }), () => refuse(reason), { kind: "opaque", reason });

/** A schema with an explicit generator for property tests. */
export const schemaWithGenerator = (gen: (rng: Rng) => unknown, schema: AnySchema): AnySchema =>
  withJson(annotate(schema, { "flick/generator": gen }), () => jsonOf(schema), flnDescOf(schema));
