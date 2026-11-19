// The AST of a schema of this runtime, the one its schema has through the
// Effect adapter, as far as Arbitrary reads it, and the declarations
// generated through a link: JSON values for Unknown, Duration and Chunk.
//
// shortcut: a schema here is its own JSON codec, so `Codec.to_schema` and
// `Schema.json_codec` give the type side: the adapter's finite numbers of a
// JSON codec, and its refusal of a JSON Result, need a distinct witness of
// the codec; add one when a program generates from such a codec
import { Duration } from "../duration.ts";
import { flickBigDecimal, flickDurationFromMillis, flickDurationFromNanos, flickDurationFromString } from "../effect/Schema.ts";
import { redactedItem, schemaDescription, type Witness } from "../schema.ts";
import { type Ast, type Filter, structNode, withChecks } from "./compile.ts";

const leaf = (tag: "boolean" | "string" | "number" | "bigint" | "decimal"): Ast => ({ tag, checks: [] });
const literal = (value: unknown): Ast => ({ tag: "literal", checks: [], literal: value });
const unsupported: Ast = { tag: "never", checks: [], what: "an unsupported Declaration" };
const linkNode = (target: Ast, decode: (value: unknown) => unknown, ...params: ReadonlyArray<Ast>): Ast =>
  ({ tag: "link", checks: [], target, params, decode });

const isInt: Filter = { number: "integer", test: (v) => Number.isSafeInteger(v) };
const isFinite: Filter = { number: "finite", test: (v) => Number.isFinite(v) };
// the runtime's check of an Int, without a constraint
const isInt32: Filter = { test: (v) => typeof v === "number" && (v | 0) === v };
const nonNegative: Filter = { test: (v) => !(v instanceof Duration && v.millis < 0) };

const ofNanos = (nanos: unknown): Duration =>
  typeof nanos !== "bigint" || nanos === 0n ? new Duration(0) : new Duration(Number(nanos) / 1_000_000, nanos);
const ofMillis = (millis: unknown): Duration => new Duration(typeof millis === "number" && millis !== 0 ? millis : 0);

/** A Duration, linked by its JSON form to a union of the infinities, nanoseconds and milliseconds. */
const durationNode = (): Ast => linkNode({
  tag: "union", checks: [], members: [
    structNode("Infinity", [], () => new Duration(Infinity)),
    structNode("NegativeInfinity", [], () => new Duration(-Infinity)),
    structNode("Nanos", [leaf("bigint")], (vs) => ofNanos(vs[0])),
    structNode("Millis", [withChecks(leaf("number"), isInt)], (vs) => ofMillis(vs[0])),
  ],
}, (value) => value);

const identity = (value: unknown): unknown => value;

/** The nodes of schemas, one per schema, so a recursive type's suspension finds the node it started from. */
export const astOf = (root: Witness): Ast => {
  const nodes = new Map<Witness, Ast>();
  let json: Ast | undefined;
  // null, finite numbers, booleans, strings, and arrays and string-keyed records of JSON values
  const jsonNode = (): Ast => {
    if (json !== undefined) return json;
    const suspend = (): Ast => ({ tag: "suspend", checks: [], thunk: () => union });
    const union: Ast = { tag: "union", checks: [], members: [
      literal(null), withChecks(leaf("number"), isFinite), leaf("boolean"), leaf("string"),
      { tag: "arrays", checks: [], elements: [], rest: suspend() },
      { tag: "record", checks: [], key: leaf("string"), value: suspend() },
    ] };
    return json = linkNode(union, identity);
  };
  const of = (schema: Witness): Ast => {
    let n = nodes.get(schema);
    if (n === undefined) nodes.set(schema, n = build(schema));
    return n;
  };
  const build = (schema: Witness): Ast => {
    const node = schemaDescription(schema);
    switch (node?.kind) {
      case "number": return leaf("number");
      case "int": return withChecks(leaf("number"), isInt32);
      case "string": return leaf("string");
      case "bool": return leaf("boolean");
      case "bigint": return leaf("bigint");
      case "unit": return literal(undefined);
      case "never": return { tag: "never", checks: [], what: "" };
      case "unknown": return jsonNode();
      case "list": return { tag: "arrays", checks: [], elements: [], rest: of(node.item) };
      case "chunk": {
        // a Chunk is an array on this runtime
        const item = of(node.item);
        return linkNode({ tag: "arrays", checks: [], elements: [], rest: item }, identity, item);
      }
      case "tuple": return { tag: "arrays", checks: [], elements: node.items.map(of), rest: undefined };
      case "record": {
        const names = node.fields.map(([name]) => name);
        return { tag: "objects", checks: [], props: node.fields.map(([, field]) => of(field)), names, make: (values) => {
          const out: Record<string, unknown> = {};
          for (const [i, name] of names.entries()) out[name] = values[i];
          return out;
        } };
      }
      case "adt": return { tag: "union", checks: [], members: node.cases.map(({ tag, fields }) =>
        structNode(tag, fields.map(of), (values) => {
          const out: Record<string, unknown> = { _tag: tag };
          for (const [i, value] of values.entries()) out[`_${i}`] = value;
          return out;
        })) };
      case "option": return { tag: "option", checks: [], item: of(node.item) };
      case "result": return { tag: "result", checks: [], value: of(node.value), error: of(node.error) };
      case "lazy": return { tag: "suspend", checks: [], thunk: () => of(node.get()) };
      case "literals": return { tag: "union", checks: [], members: node.values.map(literal) };
      // shortcut: the decoded side of an enum, also of its `{ _tag }` wire form; draw the objects when a program generates that wire form
      case "enum-tags": return { tag: "union", checks: [], members: node.pairs.map(([, value]) => literal(value)) };
      case "refined":
        return withChecks(of(node.base), { test: (v) => schema.value(v, "")._tag === "Success" });
      // `Schema.duration` checks its values, `Duration.json_millis` does not
      case "duration-millis": return schema === flickDurationFromMillis ? durationNode()
        : withChecks(durationNode(), nonNegative);
      case "transform": return schema === flickBigDecimal ? leaf("decimal") : unsupported;
      case "opaque": {
        const hidden = redactedItem(schema);
        if (hidden !== undefined) return { tag: "redacted", checks: [], item: of(hidden) };
        if (schema === flickDurationFromNanos) return durationNode();
        if (schema === flickDurationFromString) return withChecks(durationNode(), nonNegative);
        // an opaque type is Unknown, whose values are JSON
        return jsonNode();
      }
      // a witness without a description is a codec without a generator
      case "entries":
      case "block":
      case undefined:
        return unsupported;
    }
  };
  return of(root);
};
