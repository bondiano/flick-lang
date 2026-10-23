// Effect Schema issues as the program sees them: a `SchemaError` listing
// `{ path, message }`, which names the path and describes the input but
// never holds it. Before decoding, the input is checked to be inert data:
// own data properties only, and no value met again on the path that leads
// to it (a value shared by two paths is fine).
import type { Schema, SchemaAST, SchemaIssue } from "effect";

import { notesOf } from "./notes.ts";

const SCHEMA_ERROR = "Prelude.SchemaError.SchemaError";

/** `Prelude.SchemaIssue`. */
export interface Issue {
  readonly path: string;
  readonly message: string;
}

/** `Prelude.SchemaError`. */
export interface SchemaError {
  readonly _tag: typeof SCHEMA_ERROR;
  readonly _0: ReadonlyArray<Issue>;
}

/** A `SchemaError` of the given issues. */
const schemaErrorOf = (issues: ReadonlyArray<Issue>): SchemaError => ({
  _tag: SCHEMA_ERROR,
  _0: issues.map((i) => Object.freeze({ path: i.path || "(root)", message: i.message })),
});

/** A `SchemaError` of one issue. */
export const schemaError = (path: string, message: string): SchemaError => schemaErrorOf([{ path, message }]);

export const isSchemaError = (error: unknown): error is SchemaError =>
  typeof error === "object" && error !== null && Reflect.get(error, "_tag") === SCHEMA_ERROR;

export const describe = (input: unknown): string => {
  if (input === null) return "null";
  if (typeof input === "number" && !Number.isFinite(input)) return String(input);
  if (Array.isArray(input)) return "an array";
  return typeof input === "object" ? "an object" : typeof input;
};

export const at = (path: string, key: PropertyKey): string =>
  typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${String(key)}` : String(key);

/** Names of what an AST node accepts, for "expected …, got …". */
const EXPECTED: Partial<Record<SchemaAST.AST["_tag"], string>> = {
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

const expected = (ast: SchemaAST.AST): string => {
  const named = notesOf(ast).expected;
  if (named) return named;
  if (ast._tag === "Literal") return JSON.stringify(ast.literal);
  return EXPECTED[ast._tag] ?? ast._tag;
};

/** The input an issue reports, when the parser kept it. */
const inputOf = (issue: SchemaIssue.Issue): { readonly input: unknown } | undefined =>
  Object.hasOwn(issue, "input") ? { input: issue.input } : undefined;

const got = (issue: SchemaIssue.Issue): string => {
  const reported = inputOf(issue);
  return reported ? `, got ${describe(reported.input)}` : "";
};

/**
 * The issues of an Effect `Issue` tree at `path`. `encoding` tells the
 * direction: a value that fails a wire check while encoding has no wire form.
 */
const flatten = (issue: SchemaIssue.Issue, path: string, encoding: boolean, out: Array<Issue>): void => {
  switch (issue._tag) {
    case "Pointer":
      flatten(issue.issue, issue.path.reduce<string>(at, path), encoding, out);
      return;
    case "Composite":
      for (const i of issue.issues) flatten(i, path, encoding, out);
      return;
    case "Encoding":
      flatten(issue.issue, path, encoding, out);
      return;
    case "AnyOf": {
      const tags = notesOf(issue.ast)["flick/tags"];
      const reported = inputOf(issue);
      if (issue.issues.length > 0) {
        for (const i of issue.issues) flatten(i, path, encoding, out);
      } else if (tags && typeof reported?.input === "object" && reported.input !== null) {
        out.push({ path: at(path, "_tag"), message: `expected one of ${tags.join(", ")}` });
      } else {
        out.push({ path, message: `expected ${expected(issue.ast)}${got(issue)}` });
      }
      return;
    }
    case "Filter": {
      const notes = notesOf(issue.filter);
      const reported = inputOf(issue);
      if (encoding && notes["flick/wire"] && reported) {
        out.push({ path, message: `${String(reported.input)} has no JSON form` });
      } else if (notes.message) {
        out.push({ path, message: notes.message });
      } else if (notes.expected) {
        out.push({ path, message: `expected ${notes.expected}${got(issue)}` });
      } else {
        flatten(issue.issue, path, encoding, out);
      }
      return;
    }
    case "InvalidType":
      out.push({ path, message: `expected ${expected(issue.ast)}${got(issue)}` });
      return;
    case "MissingKey":
      out.push({ path, message: notesOf(issue).messageMissingKey ?? "a missing key" });
      return;
    case "UnexpectedKey":
      out.push({ path, message: "an unknown key" });
      return;
    case "InvalidValue":
    case "Forbidden":
      out.push({ path, message: notesOf(issue).message ?? "an invalid value" });
      return;
    case "OneOf":
      out.push({ path, message: "matches more than one case" });
      return;
  }
};

/** The program's `SchemaError` of an Effect `SchemaError`. */
export const toSchemaError =
  (encoding: boolean) =>
  (error: Schema.SchemaError): SchemaError => {
    const out: Array<Issue> = [];
    flatten(error.issue, "", encoding, out);
    return schemaErrorOf(out);
  };

/** Where `input` stops being inert data, as an issue; `undefined` when it is. */
export const inertProblem = (input: unknown): Issue | undefined => {
  const active = new Set<object>();
  const walk = (value: unknown, path: string): Issue | undefined => {
    if (typeof value !== "object" || value === null) return undefined;
    if (active.has(value)) return { path, message: "a cycle in the input" };
    active.add(value);
    for (const key of Object.keys(value)) {
      const d = Object.getOwnPropertyDescriptor(value, key);
      const p = Array.isArray(value) ? at(path, Number(key)) : at(path, key);
      if (d === undefined || !("value" in d)) return { path: p, message: "a getter, not data" };
      const problem = walk(d.value, p);
      if (problem) return problem;
    }
    active.delete(value);
    return undefined;
  };
  return walk(input, "");
};

/** The `SchemaError` of a problem of the input, if any. */
export const requireInert = (input: unknown): SchemaError | undefined => {
  const problem = inertProblem(input);
  return problem ? schemaError(problem.path, problem.message) : undefined;
};
