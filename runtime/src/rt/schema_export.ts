// Strict JSON Schema export for the own runtime's structural codec tree.
import { type Result, err, ok } from "../core/data.ts";
import { tag } from "../tags.ts";
import { schemaDescription, schemaError, type Codec, type ExportError, type SchemaError, type Witness } from "./schema.ts";

const EXPORT_ERROR = /*#__PURE__*/ tag("Codec.ExportError.ExportError");
/** Host schemas retain their AST for exact export when the Effect bridge is present. */
export const schemaExportHooks: {
  host: (name: string, codec: Codec) => Result<string, ExportError> | undefined;
} = { host: () => undefined };
const at = (path: string, key: string | number): string =>
  typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${key}` : key;

type JsonSchema = boolean | Readonly<Record<string, unknown>>;
class Unsupported extends Error {
  readonly hosted: boolean;
  constructor(path: string, reason: string, hosted = false) {
    super(`${path} -> JSON Schema: ${reason}`);
    this.hosted = hosted;
  }
}

const objectSchema = (fields: ReadonlyArray<readonly [string, JsonSchema]>): JsonSchema => ({
  type: "object",
  properties: Object.fromEntries(fields),
  required: fields.map(([name]) => name),
  additionalProperties: false,
});

const exportSchema = (schema: Witness, path: string, defs: Map<string, JsonSchema | null>): JsonSchema => {
  const description = schemaDescription(schema);
  if (!description) throw new Unsupported(path, "a schema made outside Flick has no canonical JSON form", true);
  switch (description.kind) {
    case "number": return { type: "number" };
    case "string": return { type: "string" };
    case "bool": return { type: "boolean" };
    case "int": return { type: "integer", minimum: -2147483648, maximum: 2147483647 };
    case "bigint": return { type: "string", pattern: "^-?[0-9]+$" };
    case "unit": return { type: "null" };
    case "never": return false;
    case "unknown": throw new Unsupported(path, "an Unknown value accepts anything, which strict export does not write");
    case "list":
    case "chunk": return { type: "array", items: exportSchema(description.item, `${path}[]`, defs) };
    case "option": return { anyOf: [exportSchema(description.item, path, defs), { type: "null" }] };
    case "result": return { oneOf: [
      objectSchema([["ok", exportSchema(description.value, at(path, "ok"), defs)]]),
      objectSchema([["err", exportSchema(description.error, at(path, "err"), defs)]]),
    ] };
    case "tuple": return {
      type: "array",
      prefixItems: description.items.map((item, i) => exportSchema(item, at(path, i), defs)),
      items: false,
    };
    case "record": return objectSchema(description.fields.map(([name, item]) =>
      [name, exportSchema(item, at(path, name), defs)] as const));
    case "adt": return { oneOf: description.cases.map(({ tag: caseTag, fields }) => {
      const name = caseTag.split("#")[0] ?? caseTag;
      return objectSchema([
        ["_tag", { const: name }],
        ...fields.map((field, i) => [`_${i}`, exportSchema(field, at(at(path, name), `_${i}`), defs)] as const),
      ]);
    }) };
    case "lazy": {
      if (!defs.has(description.name)) {
        defs.set(description.name, null);
        defs.set(description.name, exportSchema(description.get(), path, defs));
      }
      return { $ref: `#/$defs/${description.name}` };
    }
    case "opaque": throw new Unsupported(path, description.reason);
    case "literals": return description.values.length === 1
      ? { const: description.values[0] }
      : { enum: description.values };
    case "enum-tags": return { oneOf: description.pairs.map(([name]) => objectSchema([["_tag", { const: name }]])) };
    case "refined": {
      const base = exportSchema(description.base, path, defs);
      if (description.known === undefined)
        throw new Unsupported(path, `the check of ${description.name} has no JSON Schema form`);
      if (typeof base !== "object") return base;
      const known: Record<string, Readonly<Record<string, unknown>>> = {
        email: { format: "email" },
        non_empty: { minLength: 1 },
        positive: { exclusiveMinimum: 0 },
      };
      const keywords = known[description.known];
      if (!keywords) throw new Unsupported(path, `the check of ${description.name} has no JSON Schema form`);
      return { ...base, ...keywords };
    }
    case "transform": return exportSchema(description.from, path, defs);
    case "duration-millis": return { type: "number", minimum: 0 };
    case "entries": {
      if (description.unsupported) throw new Unsupported(path, "a property check has no JSON Schema form");
      return {
        type: "object", additionalProperties: exportSchema(description.value, `${path}[]`, defs),
        ...(description.min === undefined ? {} : { minProperties: description.min }),
        ...(description.max === undefined ? {} : { maxProperties: description.max }),
      };
    }
    case "block": {
      const properties = Object.fromEntries(description.fields.map(([, field]) =>
        [field.wire, exportSchema(field.codec, at(path, field.wire), defs)]));
      const required = description.fields.filter(([, field]) =>
        !field.optional && field.onMissing === undefined && field.codec.kind !== "option-nullish")
        .map(([, field]) => field.wire);
      return {
        type: "object", properties, required,
        ...(!description.ignoreExtra ? { additionalProperties: false } : {}),
      };
    }
  }
  throw new Unsupported(path, "unsupported schema");
};

const isWitness = (codec: Codec): codec is Witness =>
  "decodeAt" in codec && "encodeAt" in codec && "value" in codec;

export const codecJsonSchema = (name: string, codec: Codec): Result<string, ExportError> => {
  const defs = new Map<string, JsonSchema | null>();
  try {
    if (!isWitness(codec)) throw new Unsupported(name,
      codec.kind === "json" ? "JSON value has no JSON form" : "unsupported codec");
    const top = exportSchema(codec, name, defs);
    if (typeof top !== "object") throw new Unsupported(name, "no value has this type");
    const output = {
      $schema: "https://json-schema.org/draft/2020-12/schema",
      ...top,
      ...(defs.size > 0 ? { $defs: Object.fromEntries(defs) } : {}),
    };
    return ok(JSON.stringify(output, null, 2));
  } catch (error) {
    if (error instanceof Unsupported) {
      const hosted = error.hosted ? schemaExportHooks.host(name, codec) : undefined;
      return hosted ?? err({ _tag: EXPORT_ERROR, _0: error.message });
    }
    throw error;
  }
};


export const jsonParse = (text: string): Result<unknown, SchemaError> => {
  try {
    return ok(JSON.parse(text) as unknown);
  } catch (error) {
    return err(schemaError("", `not JSON: ${error instanceof Error ? error.message : String(error)}`));
  }
};

export const jsonStringify = (value: unknown): string | undefined => JSON.stringify(value);
