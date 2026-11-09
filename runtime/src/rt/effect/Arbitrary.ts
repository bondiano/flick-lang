// Seeded generators and property checks over the own runtime's Schema tree.
import { none, some, type Option } from "../../core/data.ts";
import { tag } from "../../tags.ts";
import { type Run } from "../fiber.ts";
import { schemaDescription, type Witness } from "../schema.ts";

type Random = () => number;
type Draw<A> = { readonly value: A } | undefined;
export interface Arbitrary<A> { readonly draw: (random: Random, size: number) => Draw<A> }
const made = <A>(draw: Arbitrary<A>["draw"]): Arbitrary<A> => ({ draw });
const drawn = <A>(value: A): Draw<A> => ({ value });
const choice = (random: Random, count: number): number => Math.floor(random() * count);

const seeded = (seed: number): Random => {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = Math.imul(state ^ (state >>> 15), state | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4_294_967_296;
  };
};

const characters = "abcdefghijklmnopqrstuvwxyz0123456789";
const word = (random: Random, size: number): string => {
  const length = choice(random, Math.max(1, Math.min(size, 12) + 1));
  let value = "";
  for (let i = 0; i < length; i++) value += characters[choice(random, characters.length)];
  return value;
};

const fromSchema = (schema: Witness, random: Random, size: number, depth = 0): Draw<unknown> => {
  if (depth > 12) return undefined;
  const node = schemaDescription(schema);
  if (node === undefined) return undefined;
  const next = (child: Witness): Draw<unknown> => fromSchema(child, random, Math.max(0, size - 1), depth + 1);
  switch (node.kind) {
    case "number": return drawn(choice(random, Math.max(1, size * 4 + 1)) - size);
    case "int": return drawn(choice(random, Math.max(1, size * 4 + 1)) - size);
    case "bigint": return drawn(BigInt(choice(random, Math.max(1, size * 4 + 1)) - size));
    case "string": return drawn(word(random, size));
    case "bool": return drawn(random() < 0.5);
    case "unit": return drawn(undefined);
    case "unknown": return drawn(null);
    case "never":
    case "opaque": return undefined;
    case "literals": return node.values.length ? drawn(node.values[choice(random, node.values.length)]) : undefined;
    case "enum-tags": return node.pairs.length ? drawn({ _tag: node.pairs[choice(random, node.pairs.length)]?.[0] }) : undefined;
    case "option": {
      if (size === 0 || random() < 0.35) return drawn(none);
      const item = next(node.item);
      return item === undefined ? undefined : drawn(some(item.value));
    }
    case "list": {
      const values: unknown[] = [];
      const length = choice(random, Math.max(1, Math.min(size, 5) + 1));
      for (let i = 0; i < length; i++) {
        const item = next(node.item);
        if (item === undefined) return undefined;
        values.push(item.value);
      }
      return drawn(values);
    }
    case "result": {
      const success = random() < 0.5;
      const field = next(success ? node.value : node.error);
      return field === undefined ? undefined : drawn(success
        ? { _tag: "Success", success: field.value } : { _tag: "Failure", failure: field.value });
    }
    case "tuple": {
      const values: unknown[] = [];
      for (const item of node.items) {
        const value = next(item);
        if (value === undefined) return undefined;
        values.push(value.value);
      }
      return drawn(values);
    }
    case "record": {
      const record: Record<string, unknown> = {};
      for (const [name, item] of node.fields) {
        const value = next(item);
        if (value === undefined) return undefined;
        record[name] = value.value;
      }
      return drawn(record);
    }
    case "adt": {
      if (!node.cases.length) return undefined;
      const selected = node.cases[choice(random, node.cases.length)];
      if (selected === undefined) return undefined;
      const value: Record<string, unknown> = { _tag: selected.tag };
      for (const [i, field] of selected.fields.entries()) {
        const item = next(field);
        if (item === undefined) return undefined;
        value[`_${i}`] = item.value;
      }
      return drawn(value);
    }
    case "lazy": return next(node.get());
    case "refined": {
      for (let i = 0; i < 100; i++) {
        const value = next(node.base);
        if (value !== undefined && schema.value(value.value, "")._tag === "Success") return value;
      }
      return undefined;
    }
    case "transform": {
      const value = next(node.from);
      if (value === undefined) return undefined;
      const wire = node.from.encode(value.value);
      if (wire._tag === "Failure") return undefined;
      const decoded = schema.decode(wire.success);
      return decoded._tag === "Success" ? drawn(decoded.success) : undefined;
    }
    case "block": return undefined;
  }
  return undefined;
};

// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the Flick type of the Schema determines A
export const schema = <A>(value: Witness): Arbitrary<A> => made((random, size) => fromSchema(value, random, size) as Draw<A>);
export const flickSchema = <A>(_options: unknown, value: Witness): Arbitrary<A> => schema(value);
export const Constant = <A>(value: A): Arbitrary<A> => made(() => drawn(value));
export const filter = <A>(predicate: (value: A) => boolean) => (source: Arbitrary<A>): Arbitrary<A> =>
  made((random, size) => {
    const value = source.draw(random, size);
    return value !== undefined && predicate(value.value) ? value : undefined;
  });

interface CheckOptions {
  readonly runs: Option<number>;
  readonly size: Option<number>;
  readonly maxDiscards: Option<number>;
  readonly maxShrinks: Option<number>;
  readonly seed: Option<number>;
  readonly replay: Option<string>;
}
const optional = <A>(value: Option<A>, fallback: A): A => value._tag === "Some" ? value.value : fallback;
const PASSED = /*#__PURE__*/ tag("Arbitrary.CheckResult.Passed");
const FALSIFIED = /*#__PURE__*/ tag("Arbitrary.CheckResult.Falsified");
const EXHAUSTED = /*#__PURE__*/ tag("Arbitrary.CheckResult.Exhausted");
const RETURNED_FALSE = /*#__PURE__*/ tag("Arbitrary.PropertyFailure.ReturnedFalse");

type CheckResult<A> =
  | { readonly _tag: string; readonly _0: { readonly runs: number; readonly discards: number } }
  | { readonly _tag: string; readonly _0: { readonly initialInput: A; readonly shrunkInput: A; readonly failure: { readonly _tag: string }; readonly runs: number; readonly discards: number; readonly shrinks: number; readonly replay: string } }
  | { readonly _tag: string; readonly _0: { readonly runs: number; readonly discards: number; readonly seed: string } };

export const flickCheckEffect = <A>(options: CheckOptions, property: (value: A) => unknown, source: Arbitrary<A>): Run =>
  (function* (): Run {
    const runs = optional(options.runs, 100);
    const seed = optional(options.seed, Date.now());
    const maxDiscards = optional(options.maxDiscards, runs * 10);
    const random = seeded(seed);
    let discards = 0;
    for (let i = 0; i < runs;) {
      const sample = source.draw(random, optional(options.size, Math.max(1, Math.ceil(i / 5))));
      if (sample === undefined) {
        if (++discards > maxDiscards) return { _tag: EXHAUSTED, _0: { runs: i, discards, seed: String(seed) } };
        continue;
      }
      const passed = yield property(sample.value);
      if (passed !== true) return { _tag: FALSIFIED, _0: {
        initialInput: sample.value, shrunkInput: sample.value, failure: { _tag: RETURNED_FALSE },
        runs: i + 1, discards, shrinks: 0, replay: `${seed}:${i}`,
      } };
      i++;
    }
    return { _tag: PASSED, _0: { runs, discards } };
  })();

export const flickFormatCheckFailure = <A>(result: CheckResult<A>): Option<string> =>
  result._tag === PASSED ? none : result._tag === EXHAUSTED ? some("Exhausted") :
    some("Property falsified");
