// The strict JSON Schema 2020-12 exporter, over the AST of an Effect Schema
// codec: the wire contract written exactly, or an `ExportError` naming the
// path of the node that has no exact form. Never `{}` or `true` for
// something it cannot state.
import { type JsonSchema as EffectJsonSchema, type Result, SchemaAST } from "effect";

import { err, ok, type Tagged } from "../data.ts";
import { at } from "./issues.ts";
import { noteOf, notesOf } from "./notes.ts";
import type { AnySchema } from "./schemas.ts";
import { tag } from "../tags.ts";

const EXPORT_ERROR = /*#__PURE__*/ tag("Codec.ExportError.ExportError");

/** `Codec.ExportError`. */
export type ExportError = Tagged<typeof EXPORT_ERROR, string>;

/** An `ExportError` with its message. */
export const exportError = (message: string): ExportError => ({ _tag: EXPORT_ERROR, _0: message });

class Unsupported extends Error {
  constructor(path: string, reason: string) {
    super(`${path} -> JSON Schema: ${reason}`);
  }
}

/** A JSON Schema, as the exporter writes it. */
type JsonSchema = boolean | { readonly [keyword: string]: unknown };

type Defs = Map<string, JsonSchema | null>;

/** The JSON Schema keywords of the checks the exporter knows. */
const KNOWN: Readonly<Record<string, JsonSchema>> = {
  email: { format: "email" },
  int: { type: "integer", minimum: -0x80000000, maximum: 0x7fffffff },
  non_empty: { minLength: 1 },
  positive: { exclusiveMinimum: 0 },
};

const TRANSFORMS = {
  pure: undefined,
  result: "a fallible transformation has no JSON Schema form",
  effect: "an effectful transformation decides acceptance at run time",
} as const;

const TYPES: ReadonlyArray<EffectJsonSchema.Type> = ["string", "number", "boolean", "array", "object", "null", "integer"];

/** Bounds that combine into one: the tighter of the two. */
const LOWER = /*#__PURE__*/ new Set(["minimum", "exclusiveMinimum", "minLength", "minItems", "minProperties"]);
const UPPER = /*#__PURE__*/ new Set(["maximum", "exclusiveMaximum", "maxLength", "maxItems", "maxProperties"]);

/** `keywords` added to `out`; a keyword already there is tightened, or joins it under `allOf`. */
const merge = (out: Record<string, unknown>, keywords: Readonly<Record<string, unknown>>): void => {
  for (const [k, v] of Object.entries(keywords)) {
    const old = out[k];
    // an integer is a number: the narrower type replaces the wider
    if (!(k in out) || (k === "type" && old === "number" && v === "integer")) out[k] = v;
    else if (typeof old === "number" && typeof v === "number" && LOWER.has(k)) out[k] = Math.max(old, v);
    else if (typeof old === "number" && typeof v === "number" && UPPER.has(k)) out[k] = Math.min(old, v);
    else if (JSON.stringify(old) !== JSON.stringify(v)) {
      const all: unknown = out["allOf"];
      out["allOf"] = Array.isArray(all) ? all.concat([{ [k]: v }]) : [{ [k]: v }];
    }
  }
};

/** The keywords Effect gives a check of a node of JSON type `type`, if exact. */
const effectKeywords = (
  check: SchemaAST.Check<never>,
  ast: SchemaAST.AST,
  type: unknown,
  path: string,
  out: Record<string, unknown>,
): void => {
  if (check._tag === "FilterGroup") {
    for (const c of check.checks) effectKeywords(c, ast, type, path, out);
    return;
  }
  const notes = notesOf(check);
  const describe = notes.expected ?? notes.message ?? "a check";
  const write = check.annotations?.toJsonSchema;
  if (write === undefined) throw new Unsupported(path, `the check "${describe}" has no JSON Schema form`);
  // a check of a transformed value constrains the decoded side, not the wire
  if (ast.encoding !== undefined) throw new Unsupported(path, `the check "${describe}" applies after a transformation`);
  let keywords: unknown;
  try {
    keywords = write({ type: TYPES.find((t) => t === type), schemas: [] });
  } catch {
    throw new Unsupported(path, `the check "${describe}" has no JSON Schema form`);
  }
  if (Array.isArray(keywords) || typeof keywords !== "object" || keywords === null) {
    throw new Unsupported(path, `the check "${describe}" has no exact JSON Schema form`);
  }
  merge(out, Object.fromEntries(Object.entries(keywords)));
};

/** The keywords of the checks of `ast`, which constrain what reaches it. */
const checks = (ast: SchemaAST.AST, path: string, type: unknown): Readonly<Record<string, unknown>> => {
  const out: Record<string, unknown> = {};
  for (const check of ast.checks ?? []) {
    const notes = notesOf(check);
    const known = notes["flick/known"];
    const keywords = known ? KNOWN[known] : undefined;
    // JSON numbers are finite: the wire type already says it
    if (notes["flick/wire"]) continue;
    if (notes["flick/pattern"]) out["pattern"] = notes["flick/pattern"];
    else if (typeof keywords === "object") Object.assign(out, keywords);
    else if (notes["flick/refined"]) throw new Unsupported(path, `the check of ${notes["flick/refined"]} has no JSON Schema form`);
    else effectKeywords(check, ast, type, path, out);
  }
  return out;
};

const object = (properties: Record<string, JsonSchema>, required: ReadonlyArray<string>, closed: boolean): JsonSchema => ({
  type: "object",
  properties,
  required,
  ...(closed ? { additionalProperties: false } : {}),
});

/** The properties of a struct, written under `wires` names. */
const objectOf = (
  ast: SchemaAST.Objects,
  path: string,
  defs: Defs,
  wires: Readonly<Record<string, string>> | undefined,
  closed: boolean,
): JsonSchema => {
  const variant = notesOf(ast)["flick/variant"];
  const entries = ast.propertySignatures.map((p) => {
    const label = String(p.name);
    const key = wires?.[label] ?? label;
    const inner = variant && key !== "_tag" ? at(at(path, variant), key) : at(path, key);
    return { key, schema: schemaOf(p.type, inner, defs), optional: p.type.context?.isOptional === true };
  });
  return object(
    Object.fromEntries(entries.map((e) => [e.key, e.schema])),
    entries.filter((e) => !e.optional).map((e) => e.key),
    closed,
  );
};

/** The wire end of `ast`: the last link of its encoding. */
const wireEnd = (ast: SchemaAST.AST): SchemaAST.AST | undefined => ast.encoding?.[ast.encoding.length - 1]?.to;

const body = (ast: SchemaAST.AST, path: string, defs: Defs): JsonSchema => {
  const notes = notesOf(ast);
  const refused = noteOf(ast, "flick/refuse");
  if (refused) throw new Unsupported(path, refused);
  const transform = noteOf(ast, "flick/transform");
  const unwritable = transform && TRANSFORMS[transform];
  if (unwritable) throw new Unsupported(path, unwritable);
  if (notes["flick/block"] && ast._tag === "Objects") {
    const closed = !notesOf(wireEnd(ast))["flick/ignoreExtra"];
    return objectOf(ast, path, defs, notes["flick/wires"], closed);
  }
  const end = wireEnd(ast);
  if (end) return schemaOf(end, path, defs);
  switch (ast._tag) {
    case "String":
      return { type: "string" };
    case "Number":
      return { type: "number" };
    case "Boolean":
      return { type: "boolean" };
    case "Null":
      return { type: "null" };
    case "Literal":
      return { const: ast.literal };
    case "Never":
      return false;
    case "Arrays": {
      const [rest, ...more] = ast.rest;
      if (ast.elements.length === 0 && rest !== undefined && more.length === 0) {
        return { type: "array", items: schemaOf(rest, `${path}[]`, defs) };
      }
      const prefixItems = ast.elements.map((e, i) => schemaOf(e, `${path}[${i}]`, defs));
      if (rest === undefined) return { type: "array", prefixItems, items: false };
      if (more.length === 0 && ast.elements.every((e) => e.context?.isOptional !== true)) {
        return { type: "array", prefixItems, items: schemaOf(rest, `${path}[]`, defs), minItems: prefixItems.length };
      }
      break;
    }
    case "Objects": {
      if (ast.indexSignatures.length === 0) return objectOf(ast, path, defs, undefined, true);
      const [index, ...others] = ast.indexSignatures;
      // a dictionary: string keys, every value of one schema
      if (index && others.length === 0 && ast.propertySignatures.length === 0 && index.parameter._tag === "String") {
        if (index.parameter.checks?.length) break;
        return { type: "object", additionalProperties: schemaOf(index.type, `${path}.*`, defs) };
      }
      break;
    }
    case "Union": {
      // an absent key is no wire value
      const members = ast.types.filter((t) => t._tag !== "Undefined");
      // a literal encoded otherwise is written as its wire end
      const literals = members.flatMap((t) => (t._tag === "Literal" && !wireEnd(t) ? [t.literal] : []));
      if (literals.length === members.length) return { enum: literals };
      return { [notes["flick/oneOf"] ? "oneOf" : "anyOf"]: members.map((t) => schemaOf(t, path, defs)) };
    }
    case "Suspend":
      return schemaOf(ast.thunk(), path, defs);
    case "Unknown":
    case "Any":
      throw new Unsupported(path, "an Unknown value accepts anything, which strict export does not write");
    default:
      break;
  }
  throw new Unsupported(path, `${notes.expected ?? ast._tag} has no JSON form`);
};

/** The JSON Schema of the wire form of `ast`. */
const schemaOf = (ast: SchemaAST.AST, path: string, defs: Defs): JsonSchema => {
  const ref = notesOf(ast)["flick/ref"];
  if (ref && ast._tag === "Suspend") {
    if (!defs.has(ref)) {
      defs.set(ref, null);
      defs.set(ref, schemaOf(ast.thunk(), path, defs));
    }
    return { $ref: `#/$defs/${ref}` };
  }
  const out = body(ast, path, defs);
  const keywords = checks(ast, path, typeof out === "object" ? out["type"] : undefined);
  if (typeof out !== "object") return out;
  // an annotation of a node with checks sits on its last check
  const { title, description } = SchemaAST.resolve(ast) ?? {};
  const merged: Record<string, unknown> = {
    ...out,
    ...(typeof title === "string" ? { title } : {}),
    ...(typeof description === "string" ? { description } : {}),
  };
  merge(merged, keywords);
  return merged;
};

/** `Codec.json_schema root codec`: the schema as JSON text, or an `ExportError`. */
export const exportJsonSchema = (root: string, codec: AnySchema): Result.Result<string, ExportError> => {
  const defs: Defs = new Map();
  try {
    const top = schemaOf(codec.ast, root, defs);
    if (typeof top !== "object") throw new Unsupported(root, "no value has this type");
    const out = {
      $schema: "https://json-schema.org/draft/2020-12/schema",
      ...top,
      ...(defs.size > 0 ? { $defs: Object.fromEntries(defs) } : {}),
    };
    return ok(JSON.stringify(out, null, 2));
  } catch (error) {
    if (error instanceof Unsupported) return err({ _tag: EXPORT_ERROR, _0: error.message });
    throw error;
  }
};
