// Schemas and codecs are inert trees of nodes that the interpreter, the
// exporters and the compiler of fast decoders walk. A schema node checks a
// value of the program; a codec node also turns a wire value into it and
// back. Every node is frozen.

const node = (fields) => Object.freeze(fields);

export const schemaNumber = node({ kind: "number" });
export const schemaString = node({ kind: "string" });
export const schemaBool = node({ kind: "bool" });
export const schemaBigint = node({ kind: "bigint" });
export const schemaUnit = node({ kind: "unit" });
export const schemaNever = node({ kind: "never" });
export const schemaUnknown = node({ kind: "unknown" });
export const schemaList = (item) => node({ kind: "list", item });
export const schemaOption = (item) => node({ kind: "option", item });
export const schemaResult = (error, value) => node({ kind: "result", error, value });
export const schemaRedacted = (item) => node({ kind: "redacted", item });
export const schemaTuple = (items) => node({ kind: "tuple", items });

/** A record: `fields` lists `[label, schema]` in label order. */
export const schemaRecord = (fields) => node({ kind: "record", fields });

/**
 * A variant type: `variants` lists `[tag, name, fields]`, the qualified
 * constructor id, its name, and the schemas of `_0`, `_1`, ….
 */
export const schemaAdt = (name, variants) => node({ kind: "adt", name, variants });

/** A refined type: `base` and a pure predicate; `known` names a check exporters know. */
export const schemaRefined = (name, base, check, known) =>
  node({ kind: "refined", name, base, check, known: known ?? null });

/** A reference resolved on use: recursive and mutually recursive types. */
export const schemaLazy = (name, get) => {
  let cached;
  return node({ kind: "lazy", name, get: () => (cached ??= get()) });
};

/** A type whose owner gives no wire form, as a scoped resource. */
export const schemaOpaque = (name, reason) => node({ kind: "opaque", name, reason });

/** A schema with an explicit generator for property tests. */
export const schemaWithGenerator = (gen, schema) => node({ ...schema, generator: gen });

export const codecString = node({ kind: "jsonString" });
export const codecNumberFinite = node({ kind: "jsonNumberFinite" });
export const codecNumberLossless = node({ kind: "jsonNumberLossless" });
export const codecBool = node({ kind: "jsonBool" });
export const codecBigint = node({ kind: "jsonBigint" });

/** The canonical JSON codec of a schema. */
export const codecJson = (schema) => node({ kind: "json", schema });

export const codecOptionNull = (item) => node({ kind: "optionNull", item });
export const codecOptionNullish = (item) => node({ kind: "optionNullish", item });
export const codecTaggedOption = (item) => node({ kind: "taggedOption", item });
export const codecList = (item) => node({ kind: "jsonList", item });

/**
 * A record codec of the `codec` DSL: `fields` lists
 * `{ label, wire, codec, optional, onMissing, onMissingEffect }`.
 */
export const codecFields = (name, fields, ignoreExtra) =>
  node({ kind: "fields", name, fields, ignoreExtra: Boolean(ignoreExtra) });

export const codecIgnoreExtra = (codec) =>
  codec.kind === "fields" ? node({ ...codec, ignoreExtra: true }) : codec;

/** `mode` is `pure`, `result` (decode may fail) or `effect` (decode is runtime). */
export const codecTransform = (mode, decode, encode, from) =>
  node({ kind: "transform", mode, decode, encode, from });

/** A whole-value check after decoding. */
export const codecCheck = (check, message, from) => node({ kind: "check", check, message, from });

/** A Redacted value as its content's wire form: an explicit opt-in. */
export const codecRedactedReveal = (item) => node({ kind: "redactedReveal", item });

/** A Redacted value written as `"<redacted>"`; it cannot be decoded. */
export const codecRedactedMasked = node({ kind: "redactedMasked" });
