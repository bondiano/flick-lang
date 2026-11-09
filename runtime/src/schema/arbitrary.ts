// Generators of values of a schema, for property tests: structural over the
// Effect Schema AST, and an explicit generator for what the structure cannot
// produce, as a refinement.
import { Option, Redacted, Result, type SchemaAST } from "effect";

import { item } from "../internal.ts";
import type { Gen, Rng } from "../stdlib/testing.ts";
import { noteOf, notesOf } from "./notes.ts";
import type { AnySchema } from "./schemas.ts";

const needs = (what: string): RangeError =>
  new RangeError(`Schema.arbitrary: ${what} needs an explicit generator (Schema.with_generator)`);

/** Fields of a struct, for preferring variants without any past the depth limit. */
const fieldCount = (ast: SchemaAST.AST): number => (ast._tag === "Objects" ? ast.propertySignatures.length - 1 : 0);

const pick = <A>(rng: Rng, xs: ReadonlyArray<A>): A => item(xs, Math.floor(rng() * xs.length));

/** The value of a declared type: `Option`, `Result` or `Redacted`. */
const declared = (ast: SchemaAST.Declaration, draw: (ast: SchemaAST.AST) => unknown, rng: Rng): unknown => {
  const [a, b] = ast.typeParameters;
  switch (notesOf(ast).expected) {
    case "Option":
      return a === undefined || rng() < 0.3 ? Option.none() : Option.some(draw(a));
    case "Result":
      if (a === undefined || b === undefined) break;
      return rng() < 0.5 ? Result.fail(draw(b)) : Result.succeed(draw(a));
    case "Redacted":
      if (a === undefined) break;
      return Redacted.make(draw(a));
    default:
      break;
  }
  throw needs(notesOf(ast).expected ?? "a declared type");
};

/** `Schema.arbitrary schema`: a generator `(rng) => value`. */
export const schemaArbitrary =
  (schema: AnySchema): Gen<unknown> =>
  (rng) => {
    const draw = (ast: SchemaAST.AST, depth: number): unknown => {
      const notes = notesOf(ast);
      // a node with checks keeps its annotations, the generator among them, on its last check
      const generator = noteOf(ast, "flick/generator");
      if (generator) return generator(rng);
      if (ast.checks?.length) {
        const refined = ast.checks.map((c) => notesOf(c)["flick/refined"]).find(Boolean);
        throw needs(refined ? `the check of ${refined}` : "a check");
      }
      const size = depth > 3 ? 0 : Math.floor(rng() * 4);
      const next = (inner: SchemaAST.AST): unknown => draw(inner, depth + 1);
      switch (ast._tag) {
        case "Number":
          return Math.floor(rng() * 2000) - 1000;
        case "String":
          return Array.from({ length: size }, () => String.fromCharCode(97 + Math.floor(rng() * 26))).join("");
        case "Boolean":
          return rng() < 0.5;
        case "BigInt":
          return BigInt(Math.floor(rng() * 2000) - 1000);
        case "Undefined":
          return undefined;
        case "Literal":
          return ast.literal;
        case "Arrays": {
          const [rest] = ast.rest;
          return rest !== undefined && ast.elements.length === 0
            ? Array.from({ length: size }, () => next(rest))
            : ast.elements.map(next);
        }
        case "Objects":
          return Object.freeze(Object.fromEntries(ast.propertySignatures.map((p) => [p.name, next(p.type)])));
        case "Union": {
          const leaves = ast.types.filter((t) => fieldCount(t) === 0);
          return next(pick(rng, depth > 3 && leaves.length > 0 ? leaves : ast.types));
        }
        case "Suspend":
          return draw(ast.thunk(), depth);
        case "Declaration":
          return declared(ast, next, rng);
        default:
          throw needs(notes.expected ?? ast._tag);
      }
    };
    return draw(schema.ast, 0);
  };
