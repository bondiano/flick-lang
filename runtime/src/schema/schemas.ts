// A schema of the program is an Effect Schema of the runtime values of a
// type. `jsonOf` gives each the codec of its canonical JSON form, built out
// of field and element codecs, so decoding a tree walks it once. Notes for
// the exporter, the formatter of issues and the generators are annotations
// under `flick/…` (see notes.ts).
import { Option, Redacted, Result, Schema, SchemaGetter, SchemaTransformation } from "effect";

import { erase } from "../internal.ts";
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

export const withJson = <S extends AnySchema>(schema: S, make: () => AnySchema): S => {
  jsonMakers.set(schema, make);
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

export const schemaNumber: AnySchema = /*#__PURE__*/ withJson(Schema.Number, () => jsonNumber);
export const schemaString: AnySchema = /*#__PURE__*/ withJson(Schema.String, () => Schema.String);
export const schemaBool: AnySchema = /*#__PURE__*/ withJson(Schema.Boolean, () => Schema.Boolean);
export const schemaBigint: AnySchema = /*#__PURE__*/ withJson(Schema.BigInt, () => jsonBigint);
export const schemaUnit: AnySchema = /*#__PURE__*/ withJson(annotate(Schema.Undefined, { expected: "()" }), () => jsonUnit);
export const schemaNever: AnySchema = /*#__PURE__*/ withJson(Schema.Never, () => Schema.Never);
export const schemaUnknown: AnySchema = /*#__PURE__*/ withJson(Schema.Unknown, () => Schema.Unknown);

export const schemaList = (item: AnySchema): AnySchema =>
  withJson(annotate(Schema.Array(item), { expected: "a list" }), () => Schema.Array(jsonOf(item)));

export const schemaTuple = (items: ReadonlyArray<AnySchema>): AnySchema =>
  withJson(annotate(Schema.Tuple(items), { expected: `a tuple of ${items.length}` }), () =>
    Schema.Tuple(items.map(jsonOf)),
  );

export const schemaOption = (item: AnySchema): AnySchema =>
  withJson(Schema.Option(item), () => Schema.OptionFromNullOr(jsonOf(item)));

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
  );

export const schemaRedacted = (item: AnySchema): AnySchema =>
  withJson(Schema.Redacted(item), () => refuse("a Redacted value has no JSON form without an explicit policy"));

/** A record: `fields` lists `[label, schema]` in label order. */
export const schemaRecord = (fields: ReadonlyArray<readonly [string, AnySchema]>): AnySchema =>
  withJson(annotate(Schema.Struct(Object.fromEntries(fields)), { expected: "a record" }), () =>
    Schema.Struct(Object.fromEntries(fields.map(([label, s]) => [label, jsonOf(s)]))),
  );

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
  return withJson(base.check(filter()), () => jsonOf(base).check(filter()));
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
  );
};

/** A type whose owner gives no wire form, as a scoped resource. */
export const schemaOpaque = (name: string, reason: string): AnySchema =>
  withJson(annotate(Schema.Unknown, { expected: name }), () => refuse(reason));

/** A schema with an explicit generator for property tests. */
export const schemaWithGenerator = (gen: (rng: Rng) => unknown, schema: AnySchema): AnySchema =>
  withJson(annotate(schema, { "flick/generator": gen }), () => jsonOf(schema));
