// The strict JSON Schema 2020-12 exporter: a codec's wire contract written
// exactly, or an `ExportError` naming the path of the node that has no
// exact form. Never `{}` or `true` for something it cannot state.

const EXPORT_ERROR = "Codec.ExportError.ExportError";

class Unsupported extends Error {
  constructor(path, reason) {
    super(`${path} -> JSON Schema: ${reason}`);
  }
}

const at = (path, key) => `${path}.${key}`;

const unlazy = (node, defs) => {
  if (node.kind !== "lazy") return { node };
  return { ref: node.name, target: node.get() };
};

const KNOWN = {
  email: { type: "string", format: "email" },
  non_empty: { type: "string", minLength: 1 },
  positive: { type: "number", exclusiveMinimum: 0 },
};

/** The JSON Schema of the canonical JSON form of a schema node. */
const schemaOf = (node, path, defs) => {
  const lazy = unlazy(node, defs);
  if (lazy.ref) {
    if (!defs.has(lazy.ref)) {
      defs.set(lazy.ref, null);
      defs.set(lazy.ref, schemaOf(lazy.target, path, defs));
    }
    return { $ref: `#/$defs/${lazy.ref}` };
  }
  const s = lazy.node;
  switch (s.kind) {
    case "number":
      return { type: "number" };
    case "string":
      return { type: "string" };
    case "bool":
      return { type: "boolean" };
    case "bigint":
      return { type: "string", pattern: "^-?[0-9]+$" };
    case "unit":
      return { type: "null" };
    case "never":
      return false;
    case "list":
      return { type: "array", items: schemaOf(s.item, `${path}[]`, defs) };
    case "tuple":
      return {
        type: "array",
        prefixItems: s.items.map((item, i) => schemaOf(item, `${path}[${i}]`, defs)),
        items: false,
      };
    case "option":
      return { anyOf: [schemaOf(s.item, path, defs), { type: "null" }] };
    case "result":
      return {
        oneOf: [
          object({ ok: schemaOf(s.value, at(path, "ok"), defs) }, ["ok"]),
          object({ err: schemaOf(s.error, at(path, "err"), defs) }, ["err"]),
        ],
      };
    case "record":
      return object(
        Object.fromEntries(s.fields.map(([label, f]) => [label, schemaOf(f, at(path, label), defs)])),
        s.fields.map(([label]) => label),
      );
    case "adt":
      return {
        oneOf: s.variants.map(([, name, fields]) =>
          object(
            Object.fromEntries([
              ["_tag", { const: name }],
              ...fields.map((f, i) => [`_${i}`, schemaOf(f, at(at(path, name), `_${i}`), defs)]),
            ]),
            ["_tag", ...fields.map((_, i) => `_${i}`)],
          ),
        ),
      };
    case "refined": {
      const known = s.known && KNOWN[s.known];
      if (!known) throw new Unsupported(path, `the check of ${s.name} has no JSON Schema form`);
      return { ...schemaOf(s.base, path, defs), ...known };
    }
    case "redacted":
      throw new Unsupported(path, "a Redacted value has no wire codec without an explicit policy");
    case "opaque":
      throw new Unsupported(path, s.reason);
    case "unknown":
      throw new Unsupported(path, "an Unknown value accepts anything, which strict export does not write");
    default:
      throw new Unsupported(path, `${s.kind} has no JSON form`);
  }
};

const object = (properties, required) => ({
  type: "object",
  properties,
  required,
  additionalProperties: false,
});

/** The JSON Schema of a codec's wire form. */
const codecOf = (codec, path, defs) => {
  switch (codec.kind) {
    case "json":
      return schemaOf(codec.schema, path, defs);
    case "jsonString":
      return { type: "string" };
    case "jsonNumberFinite":
      return { type: "number" };
    case "jsonBool":
      return { type: "boolean" };
    case "jsonBigint":
      return { type: "string", pattern: "^-?[0-9]+$" };
    case "jsonNumberLossless":
      return { anyOf: [{ type: "number" }, { enum: ["NaN", "Infinity", "-Infinity", "-0"] }] };
    case "jsonList":
      return { type: "array", items: codecOf(codec.item, `${path}[]`, defs) };
    case "optionNull":
    case "optionNullish":
      return { anyOf: [codecOf(codec.item, path, defs), { type: "null" }] };
    case "taggedOption":
      return {
        oneOf: [
          object({ _tag: { const: "None" } }, ["_tag"]),
          object({ _tag: { const: "Some" }, value: codecOf(codec.item, at(path, "value"), defs) }, ["_tag", "value"]),
        ],
      };
    case "fields": {
      const out = object(
        Object.fromEntries(codec.fields.map((f) => [f.wire, codecOf(f.codec, at(path, f.label), defs)])),
        codec.fields
          .filter((f) => !f.optional && !f.onMissing && !f.onMissingEffect && f.codec.kind !== "optionNullish")
          .map((f) => f.wire),
      );
      if (codec.ignoreExtra) delete out.additionalProperties;
      return out;
    }
    case "transform":
      if (codec.mode === "pure") return codecOf(codec.from, path, defs);
      throw new Unsupported(
        path,
        codec.mode === "effect" ? "an effectful transformation decides acceptance at run time" : "a fallible transformation has no JSON Schema form",
      );
    case "check":
      throw new Unsupported(path, `the check "${codec.message}" has no JSON Schema form`);
    case "redactedReveal":
      return codecOf(codec.item, path, defs);
    case "redactedMasked":
      return { const: "<redacted>" };
    default:
      throw new Unsupported(path, `${codec.kind} has no JSON form`);
  }
};

/** `Codec.json_schema root codec`: the schema as JSON text, or an `ExportError`. */
export const exportJsonSchema = (root, codec) => {
  const defs = new Map();
  try {
    const body = codecOf(codec, root, defs);
    const out = { $schema: "https://json-schema.org/draft/2020-12/schema", ...body };
    if (defs.size > 0) out.$defs = Object.fromEntries(defs);
    return { _tag: "Success", value: JSON.stringify(out, null, 2) };
  } catch (error) {
    if (error instanceof Unsupported) {
      return { _tag: "Failure", error: { _tag: EXPORT_ERROR, _0: error.message } };
    }
    throw error;
  }
};
