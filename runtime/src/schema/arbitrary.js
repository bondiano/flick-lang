// Generators of values of a schema, for property tests: structural over the
// Effect Schema AST, and an explicit generator for what the structure cannot
// produce, as a refinement.
import { Option, Redacted, Result } from "effect";

const needs = (what) =>
  new RangeError(`Schema.arbitrary: ${what} needs an explicit generator (Schema.with_generator)`);

/** Fields of a struct, for preferring variants without any past the depth limit. */
const fieldCount = (ast) => (ast._tag === "Objects" ? ast.propertySignatures.length - 1 : 0);

/** `Schema.arbitrary schema`: a generator `(rng) => value`. */
export const schemaArbitrary = (schema) => (rng) => {
  const draw = (ast, depth) => {
    const notes = ast.annotations ?? {};
    if (notes["flick/generator"]) return notes["flick/generator"](rng);
    if (ast.checks?.length) {
      const refined = ast.checks.map((c) => c.annotations?.["flick/refined"]).find(Boolean);
      throw needs(refined ? `the check of ${refined}` : "a check");
    }
    const size = depth > 3 ? 0 : Math.floor(rng() * 4);
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
      case "Arrays":
        return ast.rest.length === 1 && ast.elements.length === 0
          ? Array.from({ length: size }, () => draw(ast.rest[0], depth + 1))
          : ast.elements.map((e) => draw(e, depth + 1));
      case "Objects":
        return Object.freeze(Object.fromEntries(ast.propertySignatures.map((p) => [p.name, draw(p.type, depth + 1)])));
      case "Union": {
        const leaves = ast.types.filter((t) => fieldCount(t) === 0);
        const pool = depth > 3 && leaves.length > 0 ? leaves : ast.types;
        return draw(pool[Math.floor(rng() * pool.length)], depth + 1);
      }
      case "Suspend":
        return draw(ast.thunk(), depth);
      case "Declaration": {
        const [a, b] = ast.typeParameters;
        switch (notes.expected) {
          case "Option":
            return rng() < 0.3 ? Option.none() : Option.some(draw(a, depth + 1));
          case "Result":
            return rng() < 0.5 ? Result.fail(draw(b, depth + 1)) : Result.succeed(draw(a, depth + 1));
          case "Redacted":
            return Redacted.make(draw(a, depth + 1));
          default:
            break;
        }
        break;
      }
      default:
        break;
    }
    throw needs(notes.expected ?? ast._tag);
  };
  return draw(schema.ast, 0);
};
