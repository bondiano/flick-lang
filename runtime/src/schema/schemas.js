// A schema of the program is an Effect Schema of the runtime values of a
// type. `jsonOf` gives each the codec of its canonical JSON form, built out
// of field and element codecs, so decoding a tree walks it once. Notes for
// the exporter, the formatter of issues and the generators are annotations
// under `flick/…`.
import { Option, Redacted, Result, Schema, SchemaGetter, SchemaTransformation } from "effect";

/** How to build the JSON codec of a schema, by schema. */
const jsonMakers = new WeakMap();
const jsonCodecs = new WeakMap();

const withJson = (schema, make) => {
  jsonMakers.set(schema, make);
  return schema;
};

/** A codec whose wire form `reason` explains is missing, both ways. */
export const refuse = (reason) =>
  Schema.Unknown.annotate({ "flick/refuse": reason }).pipe(
    Schema.decodeTo(Schema.Unknown, {
      decode: SchemaGetter.forbidden(() => reason),
      encode: SchemaGetter.forbidden(() => reason),
    }),
  );

/** The codec of the canonical JSON form of `schema`, made once. */
export const jsonOf = (schema) => {
  let codec = jsonCodecs.get(schema);
  if (codec === undefined) {
    const make = jsonMakers.get(schema);
    codec = make ? make() : refuse("a schema made outside Flick has no canonical JSON form");
    jsonCodecs.set(schema, codec);
  }
  return codec;
};

/** `value` mapped both ways, onto a shallow check of the decoded side. */
const mapped = (to, decode, encode) => (from) =>
  from.pipe(Schema.decodeTo(to, SchemaTransformation.transform({ decode, encode })));

export const anyOption = Schema.declare(Option.isOption, { expected: "an Option" });
export const anyResult = Schema.declare(Result.isResult, { expected: "a Result" });
export const anyRedacted = Schema.declare(Redacted.isRedacted, { expected: "a Redacted value" });

const finite = Schema.makeFilter((n) => Number.isFinite(n), { expected: "a finite number", "flick/wire": true });

/** A JSON number: finite both ways. */
export const jsonNumber = Schema.Number.annotate({ expected: "a finite number" }).check(finite);

const digits = Schema.makeFilter((s) => /^-?\d+$/.test(s), {
  message: "expected a string of digits",
  "flick/pattern": "^-?[0-9]+$",
});

/** A BigInt as a string of digits. */
export const jsonBigint = Schema.String.annotate({ expected: "a string of digits" })
  .check(digits)
  .pipe(mapped(Schema.BigInt, (s) => BigInt(s), (n) => String(n)));

const jsonUnit = Schema.Null.pipe(mapped(Schema.Undefined, () => undefined, () => null));

export const schemaNumber = withJson(Schema.Number, () => jsonNumber);
export const schemaString = withJson(Schema.String, () => Schema.String);
export const schemaBool = withJson(Schema.Boolean, () => Schema.Boolean);
export const schemaBigint = withJson(Schema.BigInt, () => jsonBigint);
export const schemaUnit = withJson(Schema.Undefined.annotate({ expected: "()" }), () => jsonUnit);
export const schemaNever = withJson(Schema.Never, () => Schema.Never);
export const schemaUnknown = withJson(Schema.Unknown, () => Schema.Unknown);

export const schemaList = (item) =>
  withJson(Schema.Array(item).annotate({ expected: "a list" }), () => Schema.Array(jsonOf(item)));

export const schemaTuple = (items) =>
  withJson(Schema.Tuple(items).annotate({ expected: `a tuple of ${items.length}` }), () =>
    Schema.Tuple(items.map(jsonOf)),
  );

export const schemaOption = (item) =>
  withJson(Schema.Option(item), () => Schema.OptionFromNullOr(jsonOf(item)));

export const schemaResult = (error, value) =>
  withJson(Schema.Result(value, error), () =>
    Schema.Union([Schema.Struct({ ok: jsonOf(value) }), Schema.Struct({ err: jsonOf(error) })])
      .annotate({ expected: "an object with `ok` or `err`", "flick/oneOf": true })
      .pipe(
        mapped(
          anyResult,
          (v) => ("ok" in v ? Result.succeed(v.ok) : Result.fail(v.err)),
          (r) => (Result.isSuccess(r) ? { ok: r.success } : { err: r.failure }),
        ),
      ),
  );

export const schemaRedacted = (item) =>
  withJson(Schema.Redacted(item), () =>
    refuse("a Redacted value has no JSON form without an explicit policy"),
  );

/** A record: `fields` lists `[label, schema]` in label order. */
export const schemaRecord = (fields) =>
  withJson(Schema.Struct(Object.fromEntries(fields)).annotate({ expected: "a record" }), () =>
    Schema.Struct(Object.fromEntries(fields.map(([label, s]) => [label, jsonOf(s)]))),
  );

const positional = (fields, each) => Object.fromEntries(fields.map((f, i) => [`_${i}`, each(f)]));

/** The wire `_tag` of a variant: its name, decoded to its qualified id. */
const tagCodec = (name, tag) =>
  Schema.Literal(name).pipe(mapped(Schema.Literal(tag), () => tag, () => name));

/**
 * A variant type: `variants` lists `[tag, name, fields]`, the qualified
 * constructor id, its name, and the schemas of `_0`, `_1`, ….
 */
export const schemaAdt = (name, variants) => {
  const names = variants.map(([, n]) => n);
  const type = Schema.Union(
    variants.map(([tag, , fields]) => Schema.Struct({ _tag: Schema.Literal(tag), ...positional(fields, (f) => f) })),
  ).annotate({ expected: `a ${name}` });
  return withJson(type, () =>
    Schema.Union(
      variants.map(([tag, n, fields]) =>
        Schema.Struct({ _tag: tagCodec(n, tag), ...positional(fields, jsonOf) }).annotate({ "flick/variant": n }),
      ),
    ).annotate({ expected: "an object with a `_tag`", "flick/tags": names, "flick/oneOf": true }),
  );
};

/** A refined type: `base` and a pure predicate; `known` names a check exporters know. */
export const schemaRefined = (name, base, check, known) => {
  const filter = () => Schema.makeFilter((v) => check(v), {
      message: `not a valid ${name}`,
      "flick/known": known ?? null,
      "flick/refined": name,
    });
  return withJson(base.check(filter()), () => jsonOf(base).check(filter()));
};

/** A reference resolved on use: recursive and mutually recursive types. */
export const schemaLazy = (name, get) => {
  // a generic type's witness makes a new schema per call: resolve it once
  let target;
  const once = () => (target ??= get());
  return withJson(Schema.suspend(once).annotate({ "flick/ref": name }), () =>
    Schema.suspend(() => jsonOf(once())).annotate({ "flick/ref": name }),
  );
};

/** A type whose owner gives no wire form, as a scoped resource. */
export const schemaOpaque = (name, reason) =>
  withJson(Schema.Unknown.annotate({ expected: name }), () => refuse(reason));

/** A schema with an explicit generator for property tests. */
export const schemaWithGenerator = (gen, schema) =>
  withJson(schema.annotate({ "flick/generator": gen }), () => jsonOf(schema));
