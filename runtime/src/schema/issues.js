// Effect Schema issues as the program sees them: a `SchemaError` listing
// `{ path, message }`, which names the path and describes the input but
// never holds it. Before decoding, the input is checked to be inert data:
// own data properties only, and no value met again on the path that leads
// to it (a value shared by two paths is fine).
import { Effect } from "effect";

const SCHEMA_ERROR = "Prelude.SchemaError.SchemaError";

/** A `SchemaError` of the given issues. */
const schemaErrorOf = (issues) => ({ _tag: SCHEMA_ERROR, _0: issues.map((i) => Object.freeze(i)) });

/** A `SchemaError` of one issue. */
export const schemaError = (path, message) => schemaErrorOf([{ path: path || "(root)", message }]);

export const describe = (input) => {
  if (input === null) return "null";
  if (typeof input === "number" && !Number.isFinite(input)) return String(input);
  if (Array.isArray(input)) return "an array";
  return typeof input === "object" ? "an object" : typeof input;
};

export const at = (path, key) =>
  typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${String(key)}` : String(key);

/** Names of what an AST node accepts, for "expected …, got …". */
const EXPECTED = {
  String: "a string",
  Number: "a number",
  Boolean: "a boolean",
  BigInt: "a BigInt",
  Undefined: "()",
  Null: "null",
  Arrays: "an array",
  Objects: "an object",
  Never: "nothing (Never)",
};

const expected = (ast) => {
  const named = ast.annotations?.expected;
  if (named) return named;
  if (ast._tag === "Literal") return JSON.stringify(ast.literal);
  return EXPECTED[ast._tag] ?? ast._tag;
};

const has = (issue) => Object.hasOwn(issue, "input");

/**
 * The issues of an Effect `Issue` tree at `path`. `encoding` tells the
 * direction: a value that fails a wire check while encoding has no wire form.
 */
const flatten = (issue, path, encoding, out) => {
  switch (issue._tag) {
    case "Pointer":
      flatten(issue.issue, issue.path.reduce(at, path), encoding, out);
      return;
    case "Composite":
      for (const i of issue.issues) flatten(i, path, encoding, out);
      return;
    case "Encoding":
      flatten(issue.issue, path, encoding, out);
      return;
    case "AnyOf": {
      const tags = issue.ast.annotations?.["flick/tags"];
      if (issue.issues.length > 0) {
        for (const i of issue.issues) flatten(i, path, encoding, out);
      } else if (tags && has(issue) && typeof issue.input === "object" && issue.input !== null) {
        out.push({ path: at(path, "_tag"), message: `expected one of ${tags.join(", ")}` });
      } else {
        out.push({ path, message: `expected ${expected(issue.ast)}${has(issue) ? `, got ${describe(issue.input)}` : ""}` });
      }
      return;
    }
    case "Filter": {
      const notes = issue.filter.annotations ?? {};
      if (encoding && notes["flick/wire"] && has(issue)) {
        out.push({ path, message: `${String(issue.input)} has no JSON form` });
      } else if (notes.message) {
        out.push({ path, message: notes.message });
      } else if (notes.expected) {
        out.push({ path, message: `expected ${notes.expected}${has(issue) ? `, got ${describe(issue.input)}` : ""}` });
      } else {
        flatten(issue.issue, path, encoding, out);
      }
      return;
    }
    case "InvalidType":
      out.push({ path, message: `expected ${expected(issue.ast)}${has(issue) ? `, got ${describe(issue.input)}` : ""}` });
      return;
    case "MissingKey":
      out.push({ path, message: issue.annotations?.messageMissingKey ?? "a missing key" });
      return;
    case "UnexpectedKey":
      out.push({ path, message: "an unknown key" });
      return;
    case "InvalidValue":
    case "Forbidden":
      out.push({ path, message: issue.annotations?.message ?? "an invalid value" });
      return;
    case "OneOf":
      out.push({ path, message: "matches more than one case" });
      return;
    default:
      out.push({ path, message: String(issue) });
  }
};

/** The `SchemaError` of an Effect `SchemaError`; anything else is kept. */
export const toSchemaError = (error, encoding) => {
  if (error?._tag !== "SchemaError" || !error.issue) return error;
  const out = [];
  flatten(error.issue, "", encoding, out);
  return schemaErrorOf(out.map((i) => ({ path: i.path || "(root)", message: i.message })));
};

/** Where `input` stops being inert data, as an issue; `undefined` when it is. */
export const inertProblem = (input) => {
  const active = new Set();
  const walk = (value, path) => {
    if (typeof value !== "object" || value === null) return undefined;
    if (active.has(value)) return { path, message: "a cycle in the input" };
    active.add(value);
    for (const key of Object.keys(value)) {
      const d = Object.getOwnPropertyDescriptor(value, key);
      const p = Array.isArray(value) ? at(path, Number(key)) : at(path, key);
      if (!("value" in d)) return { path: p, message: "a getter, not data" };
      const problem = walk(d.value, p);
      if (problem) return problem;
    }
    active.delete(value);
    return undefined;
  };
  return walk(input, "");
};

/** Fails with the `SchemaError` of a problem of the input, if any. */
export const requireInert = (input) => {
  const problem = inertProblem(input);
  return problem ? Effect.fail(schemaError(problem.path, problem.message)) : undefined;
};
