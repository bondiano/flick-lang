// Generators of values of a schema, for property tests: structural, and an
// explicit generator for what the structure cannot produce, as a refinement.
import { Option, Redacted, Result } from "effect";

const unlazy = (node) => (node.kind === "lazy" ? unlazy(node.get()) : node);

/** `Schema.arbitrary schema`: a generator `(rng) => value`. */
export const schemaArbitrary = (schema) => (rng) => {
  const draw = (node, depth) => {
    const s = unlazy(node);
    if (s.generator) return s.generator(rng);
    const size = depth > 3 ? 0 : Math.floor(rng() * 4);
    switch (s.kind) {
      case "number":
        return Math.floor(rng() * 2000) - 1000;
      case "string":
        return Array.from({ length: size }, () => String.fromCharCode(97 + Math.floor(rng() * 26))).join("");
      case "bool":
        return rng() < 0.5;
      case "bigint":
        return BigInt(Math.floor(rng() * 2000) - 1000);
      case "unit":
        return undefined;
      case "list":
        return Array.from({ length: size }, () => draw(s.item, depth + 1));
      case "tuple":
        return s.items.map((item) => draw(item, depth + 1));
      case "option":
        return rng() < 0.3 ? Option.none() : Option.some(draw(s.item, depth + 1));
      case "result":
        return rng() < 0.5 ? Result.fail(draw(s.error, depth + 1)) : Result.succeed(draw(s.value, depth + 1));
      case "redacted":
        return Redacted.make(draw(s.item, depth + 1));
      case "record":
        return Object.freeze(Object.fromEntries(s.fields.map(([l, f]) => [l, draw(f, depth + 1)])));
      case "adt": {
        // past the depth limit, prefer a variant without fields
        const leaves = s.variants.filter(([, , fields]) => fields.length === 0);
        const pool = depth > 3 && leaves.length > 0 ? leaves : s.variants;
        const [tag, , fields] = pool[Math.floor(rng() * pool.length)];
        return Object.freeze(Object.fromEntries([["_tag", tag], ...fields.map((f, i) => [`_${i}`, draw(f, depth + 1)])]));
      }
      default:
        throw new RangeError(
          `Schema.arbitrary: ${s.kind === "refined" ? `the check of ${s.name}` : s.kind} needs an explicit generator (Schema.with_generator)`,
        );
    }
  };
  return draw(schema, 0);
};
