// The strict JSON Schema 2020-12 exporter, over the AST of an Effect Schema
// codec: the wire contract written exactly, or an `ExportError` naming the
// path of the node that has no exact form. Never `{}` or `true` for
// something it cannot state.
import { at } from "./issues.js";

const EXPORT_ERROR = "Codec.ExportError.ExportError";

class Unsupported extends Error {
  constructor(path, reason) {
    super(`${path} -> JSON Schema: ${reason}`);
  }
}

/** The JSON Schema keywords of the checks the exporter knows. */
const KNOWN = {
  email: { format: "email" },
  non_empty: { minLength: 1 },
  positive: { exclusiveMinimum: 0 },
};

const TRANSFORMS = {
  result: "a fallible transformation has no JSON Schema form",
  effect: "an effectful transformation decides acceptance at run time",
};

/** The keywords of the checks of `ast`, which constrain what reaches it. */
const checks = (ast, path) => {
  let out = {};
  for (const check of ast.checks ?? []) {
    const notes = check.annotations ?? {};
    // JSON numbers are finite: the wire type already says it
    if (notes["flick/wire"]) continue;
    if (notes["flick/pattern"]) out = { ...out, pattern: notes["flick/pattern"] };
    else if (KNOWN[notes["flick/known"]]) out = { ...out, ...KNOWN[notes["flick/known"]] };
    else if (notes["flick/refined"]) throw new Unsupported(path, `the check of ${notes["flick/refined"]} has no JSON Schema form`);
    else throw new Unsupported(path, `the check "${notes.message ?? "a check"}" has no JSON Schema form`);
  }
  return out;
};

const object = (properties, required, closed) => ({
  type: "object",
  properties,
  required,
  ...(closed ? { additionalProperties: false } : {}),
});

/** The properties of a struct, written under `wires` names. */
const objectOf = (ast, path, defs, wires, closed) => {
  const variant = ast.annotations?.["flick/variant"];
  const entries = ast.propertySignatures.map((p) => {
    const key = wires?.[p.name] ?? p.name;
    const inner = variant && key !== "_tag" ? at(at(path, variant), key) : at(path, key);
    return [key, schemaOf(p.type, inner, defs), p.type.context?.isOptional === true];
  });
  return object(
    Object.fromEntries(entries.map(([key, s]) => [key, s])),
    entries.filter(([, , optional]) => !optional).map(([key]) => key),
    closed,
  );
};

/** The wire end of `ast`: the last link of its encoding. */
const wireEnd = (ast) => ast.encoding?.[ast.encoding.length - 1].to;

const body = (ast, path, defs) => {
  const notes = ast.annotations ?? {};
  if (notes["flick/refuse"]) throw new Unsupported(path, notes["flick/refuse"]);
  if (TRANSFORMS[notes["flick/transform"]]) throw new Unsupported(path, TRANSFORMS[notes["flick/transform"]]);
  if (notes["flick/block"]) {
    const end = wireEnd(ast);
    const closed = !end?.annotations?.["flick/ignoreExtra"];
    return objectOf(ast, path, defs, notes["flick/wires"], closed);
  }
  if (ast.encoding) return schemaOf(wireEnd(ast), path, defs);
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
    case "Arrays":
      if (ast.elements.length === 0 && ast.rest.length === 1) {
        return { type: "array", items: schemaOf(ast.rest[0], `${path}[]`, defs) };
      }
      if (ast.rest.length === 0) {
        return {
          type: "array",
          prefixItems: ast.elements.map((e, i) => schemaOf(e, `${path}[${i}]`, defs)),
          items: false,
        };
      }
      break;
    case "Objects":
      if (ast.indexSignatures.length === 0) return objectOf(ast, path, defs, undefined, true);
      break;
    case "Union": {
      // an absent key is no wire value
      const members = ast.types.filter((t) => t._tag !== "Undefined");
      if (members.every((t) => t._tag === "Literal")) return { enum: members.map((t) => t.literal) };
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
const schemaOf = (ast, path, defs) => {
  const ref = ast.annotations?.["flick/ref"];
  if (ref && ast._tag === "Suspend") {
    if (!defs.has(ref)) {
      defs.set(ref, null);
      defs.set(ref, schemaOf(ast.thunk(), path, defs));
    }
    return { $ref: `#/$defs/${ref}` };
  }
  const keywords = checks(ast, path);
  const out = body(ast, path, defs);
  return Object.keys(keywords).length > 0 ? { ...out, ...keywords } : out;
};

/** `Codec.json_schema root codec`: the schema as JSON text, or an `ExportError`. */
export const exportJsonSchema = (root, codec) => {
  const defs = new Map();
  try {
    const out = { $schema: "https://json-schema.org/draft/2020-12/schema", ...schemaOf(codec.ast, root, defs) };
    if (defs.size > 0) out.$defs = Object.fromEntries(defs);
    return { _tag: "Success", value: JSON.stringify(out, null, 2) };
  } catch (error) {
    if (error instanceof Unsupported) {
      return { _tag: "Failure", error: { _tag: EXPORT_ERROR, _0: error.message } };
    }
    throw error;
  }
};
