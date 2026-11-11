export { jsonCodec as Json } from "../schema.ts";
export { schemaList as flickChunkJson } from "../schema.ts";
import { err, ok, type HashDict, type Result } from "../../core/data.ts";
import {
  eqHashCanonical,
  hashMapFromList,
  hashMapToList,
  hashSetFromList,
  hashSetToList,
  type FlickHashMap,
  type FlickHashSet,
} from "../collections.ts";
import { decimalFromString, decimalToString, type Decimal } from "../decimal.ts";
import { Duration } from "../duration.ts";
import { type Codec, type SchemaError, type Witness, schemaError, schemaList, schemaNumber, schemaString, schemaTupleOf, withWire, witness } from "../schema.ts";

const durationMillis = /*#__PURE__*/ witness("duration", (input, path) => {
  const number = schemaNumber.decodeAt(input, path);
  if (number._tag === "Failure") return number;
  return typeof input === "number" && input >= 0 ? ok(new Duration(input))
    : err(schemaError(path, "expected a value greater than or equal to 0, got number"));
}, (value, path) => {
  if (!(value instanceof Duration)) return err(schemaError(path, "expected a non-negative duration"));
  const number = schemaNumber.encodeAt(value.millis, path);
  if (number._tag === "Failure") return number;
  return value.millis >= 0 ? number
    : err(schemaError(path, "expected a value greater than or equal to 0, got number"));
}, undefined, { kind: "duration-millis" });

/** Canonical JSON witnesses for the native runtime's value representations. */
export const flickDuration: Witness = durationMillis;
export const flickDurationFromMillis: Witness = durationMillis;

const isDecimal = (value: unknown): value is Decimal =>
  typeof value === "object" && value !== null && typeof Reflect.get(value, "value") === "bigint"
    && typeof Reflect.get(value, "scale") === "number";

export const flickBigDecimal: Witness = /*#__PURE__*/ withWire(witness("decimal", (input, path) => {
  if (typeof input !== "string") return err(schemaError(path, "expected a decimal string"));
  const parsed = decimalFromString(input);
  return parsed._tag === "Success" ? ok(parsed.success) : err(schemaError(path, parsed.failure._0));
}, (value, path) => isDecimal(value) ? ok(decimalToString(value)) : err(schemaError(path, "expected a decimal"))), schemaString);
export const flickBigDecimalFromString: Witness = flickBigDecimal;
export const flickChunk = (item: Witness): Witness => {
  const list = schemaList(item);
  return witness("chunk", list.decodeAt, list.encodeAt, list.value, { kind: "chunk", item });
};

/** JSON collection codecs retain the native collection representation. */
const isHashMap = (value: unknown): value is FlickHashMap<unknown, unknown> =>
  typeof value === "object" && value !== null && "entries" in value && Array.isArray(value.entries);
const isHashSet = (value: unknown): value is FlickHashSet<unknown> =>
  typeof value === "object" && value !== null && "map" in value && isHashMap(value.map);
const isPairs = (value: unknown): value is ReadonlyArray<readonly [unknown, unknown]> =>
  Array.isArray(value) && value.every((entry: unknown) => Array.isArray(entry) && entry.length === 2);

export const flickHashSetJson = (dict: HashDict<unknown>, item: Witness): Witness => {
  const array = schemaList(item);
  return withWire(witness("hash-set", (input, path) => {
    const decoded = array.decodeAt(input, path);
    if (decoded._tag === "Failure") return decoded;
    return Array.isArray(decoded.success) ? ok(hashSetFromList(eqHashCanonical(dict), decoded.success))
      : err(schemaError(path, "expected an array"));
  }, (value, path) => {
    return isHashSet(value) ? array.encodeAt(hashSetToList(value), path)
      : err(schemaError(path, "expected a HashSet"));
  }), array);
};

export const flickHashMapJson = (dict: HashDict<unknown>, key: Witness, value: Witness): Witness => {
  const array = schemaList(schemaTupleOf([key, value]));
  return withWire(witness("hash-map", (input, path) => {
    const decoded = array.decodeAt(input, path);
    if (decoded._tag === "Failure") return decoded;
    return isPairs(decoded.success) ? ok(hashMapFromList(eqHashCanonical(dict), decoded.success))
      : err(schemaError(path, "expected an array of pairs"));
  }, (input, path) => {
    return isHashMap(input) ? array.encodeAt(hashMapToList(input), path)
      : err(schemaError(path, "expected a HashMap"));
  }), array);
};

type EntryCheck = (object: Record<string, unknown>, path: string) => Result<unknown, SchemaError>;
interface EntryBounds { readonly min?: number; readonly max?: number; readonly unsupported: boolean }
const entryCodecs = /*#__PURE__*/ new WeakMap<Witness, {
  readonly value: Witness; readonly checks: ReadonlyArray<EntryCheck>; readonly bounds: EntryBounds;
}>();
const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const entryCodec = (value: Witness, checks: ReadonlyArray<EntryCheck>, bounds: EntryBounds = { unsupported: false }): Witness => {
  const checked = (object: Record<string, unknown>, path: string): Result<unknown, SchemaError> => {
    for (const check of checks) {
      const result = check(object, path);
      if (result._tag === "Failure") return result;
    }
    return ok(object);
  };
  const codec = witness("entries", (input, path) => {
    if (!isObject(input)) return err(schemaError(path, "expected an object"));
    const check = checked(input, path);
    if (check._tag === "Failure") return check;
    const out: Array<[string, unknown]> = [];
    for (const [key, wire] of Object.entries(input)) {
      const decoded = value.decodeAt(wire, path ? `${path}.${key}` : key);
      if (decoded._tag === "Failure") return decoded;
      out.push([key, decoded.success]);
    }
    return ok(out);
  }, (input, path) => {
    if (!isPairs(input) || input.some(([key]) => typeof key !== "string"))
      return err(schemaError(path, "expected a list of string key pairs"));
    const out: Record<string, unknown> = {};
    for (const [key, item] of input) {
      if (typeof key !== "string") return err(schemaError(path, "expected a string key"));
      const encoded = value.encodeAt(item, path ? `${path}.${key}` : key);
      if (encoded._tag === "Failure") return encoded;
      Object.defineProperty(out, key, { value: encoded.success, enumerable: true, writable: true, configurable: true });
    }
    const check = checked(out, path);
    return check._tag === "Failure" ? check : ok(out);
  }, undefined, { kind: "entries", value, ...bounds });
  entryCodecs.set(codec, { value, checks, bounds });
  return codec;
};
export const flickEntries = (value: Witness): Witness => entryCodec(value, []);
const withEntryCheck = (codec: Witness, check: EntryCheck, range?: { readonly min?: number; readonly max?: number }): Witness => {
  const previous = entryCodecs.get(codec);
  if (!previous) throw new TypeError("a property check applies to a codec made by `Codec.entries`");
  const { bounds } = previous;
  const min = range?.min === undefined ? bounds.min : Math.max(bounds.min ?? range.min, range.min);
  const max = range?.max === undefined ? bounds.max : Math.min(bounds.max ?? range.max, range.max);
  return entryCodec(previous.value, [...previous.checks, check], {
    ...(min === undefined ? {} : { min }),
    ...(max === undefined ? {} : { max }),
    unsupported: bounds.unsupported || range === undefined,
  });
};
const countCheck = (valid: (count: number) => boolean, message: string): EntryCheck => (object, path) =>
  valid(Object.keys(object).length) ? ok(object) : err(schemaError(path, message));
export const flickIsMinProperties = (n: number, codec: Witness): Witness =>
  withEntryCheck(codec, countCheck((count) => count >= n, `expected a value with at least ${n} entries, got an object`), { min: n });
export const flickIsMaxProperties = (n: number, codec: Witness): Witness =>
  withEntryCheck(codec, countCheck((count) => count <= n, `expected a value with at most ${n} entries, got an object`), { max: n });
export const flickIsBetweenProperties = (min: number, max: number, codec: Witness): Witness =>
  withEntryCheck(codec, countCheck((count) => count >= min && count <= max,
    `expected a value with between ${min} and ${max} entries, got an object`), { min, max });
export const flickIsPropertyNames = (key: Witness, codec: Witness): Witness =>
  withEntryCheck(codec, (object, path) => Object.keys(object).every((name) => key.decode(name)._tag === "Success")
    ? ok(object) : err(schemaError(path, "expected an object with property names matching the schema, got an object")));

export const toDifferJsonPatch = <A>(codec: Codec<A>): { readonly codec: Codec<A> } => ({ codec });
export const flickToType = <A>(codec: A): A => codec;
