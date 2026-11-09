// Pure codecs and the field policies of a Flick `codec` block.
import { err, none, ok, some } from "../core/data.ts";
import { currentOwner } from "./fiber.ts";
import { type Redacted, redactedMake, redactedReveal } from "./redacted.ts";
import {
  type Codec,
  type SchemaError,
  type Witness,
  codecDecodeResult,
  schemaBool,
  schemaError,
  schemaInt,
  schemaNumber,
  schemaOption,
  schemaLiterals,
  schemaString,
  codecTransformPure,
  withWire,
  withBlock,
  witness,
} from "./schema.ts";

export const codecString = schemaString;
export const codecBool = schemaBool;
export const codecInt = schemaInt;
export const codecNumberFinite = schemaNumber;
export const codecNumberLossless: Witness = /*#__PURE__*/ witness("number-lossless",
  (input, path) => {
    if (typeof input === "number") return ok(input);
    if (input === "NaN") return ok(Number.NaN);
    if (input === "Infinity") return ok(Infinity);
    if (input === "-Infinity") return ok(-Infinity);
    if (input === "-0") return ok(-0);
    return err(schemaError(path, `expected a number, got ${describe(input)}`));
  }, (value, path) => {
    if (typeof value !== "number") return err(schemaError(path, `expected a number, got ${describe(value)}`));
    if (Object.is(value, -0)) return ok("-0");
    if (Number.isNaN(value)) return ok("NaN");
    if (value === Infinity) return ok("Infinity");
    if (value === -Infinity) return ok("-Infinity");
    return ok(value);
  });
export const codecOptionNull = schemaOption;
const isRedacted = (value: unknown): value is Redacted<unknown> =>
  typeof value === "object" && value !== null && typeof Reflect.get(value, "toJSON") === "function";
export const codecRedactedReveal = (item: Witness): Witness =>
  codecTransformPure(redactedMake, (value) => {
    if (!isRedacted(value)) throw new TypeError("not a Redacted value");
    return redactedReveal(value);
  }, item);
export const codecRedactedMasked: Witness = /*#__PURE__*/ withWire(witness("redacted-masked",
  (_value, path) => err(schemaError(path, "a masked Redacted value cannot be decoded")),
  () => ok("<redacted>")), schemaLiterals(schemaString, undefined, ["<redacted>"]));

export const codecOptionNullish = (item: Witness): Witness => witness("option-nullish",
  (input, path) => input === null || input === undefined
    ? ok(none)
    : map(item.decodeAt(input, path), some),
  (value, path) => {
    if (!isRecord(value)) return err(schemaError(path, "expected an Option"));
    if (value["_tag"] === "None") return ok(null);
    if (value["_tag"] === "Some") return item.encodeAt(value["value"], path);
    return err(schemaError(path, "expected an Option"));
  });

export const codecTaggedOption = (item: Witness): Witness => witness("tagged-option",
  (input, path) => {
    if (!isRecord(input)) return err(schemaError(path, `expected an object with \`_tag\` None or Some, got ${describe(input)}`));
    if (input["_tag"] === "None") return ok(none);
    if (input["_tag"] === "Some") {
      if (!Object.hasOwn(input, "value")) return err(schemaError(at(path, "value"), "a missing key"));
      return map(item.decodeAt(input["value"], at(path, "value")), some);
    }
    return err(schemaError(at(path, "_tag"), "expected one of None, Some"));
  }, (value, path) => {
    if (!isRecord(value)) return err(schemaError(path, "expected an Option"));
    if (value["_tag"] === "None") return ok({ _tag: "None" });
    if (value["_tag"] === "Some")
      return map(item.encodeAt(value["value"], at(path, "value")), (wire) => ({ _tag: "Some", value: wire }));
    return err(schemaError(path, "expected an Option"));
  });

interface FieldSpec {
  readonly wire: string;
  readonly codec: Witness;
  readonly optional: boolean;
  readonly onMissing?: (unit: undefined) => unknown;
}
type BlockFields = ReadonlyArray<readonly [string, FieldSpec]>;

export const dslField = (wire: string, codec: Witness): FieldSpec => Object.freeze({ wire, codec, optional: false });
export const dslOptionalField = (wire: string, codec: Witness): FieldSpec => Object.freeze({ wire, codec, optional: true });
export const dslOnMissing = (factory: (unit: undefined) => unknown, field: FieldSpec): FieldSpec =>
  Object.freeze({ ...field, onMissing: factory });

export const codecFieldsEmpty: BlockFields = /*#__PURE__*/ Object.freeze([]);
export const codecField = (label: string, field: FieldSpec, rest: BlockFields): BlockFields =>
  Object.freeze([...rest, [label, field] as const]);

const blocks = /*#__PURE__*/ new WeakMap<Codec, { readonly name: string; readonly fields: BlockFields; readonly ignoreExtra: boolean }>();

const makeBlock = (name: string, fields: BlockFields, ignoreExtra: boolean): Witness => {
  const wires = new Set<string>();
  for (const [, field] of fields) {
    if (wires.has(field.wire)) throw new Error(`two fields of the codec of ${name} use the wire name "${field.wire}"`);
    wires.add(field.wire);
  }
  const codec = witness(`block:${name}`, (input, path) => {
    if (!isRecord(input)) return err(schemaError(path, `expected an object, got ${describe(input)}`));
    if (!ignoreExtra) {
      for (const wire of Object.keys(input))
        if (!wires.has(wire)) return err(schemaError(at(path, wire), "an unknown key"));
    }
    const out: Record<string, unknown> = {};
    for (const [label, field] of fields) {
      const where = at(path, field.wire);
      if (!Object.hasOwn(input, field.wire)) {
        if (field.optional) {
          out[label] = none;
          continue;
        }
        if (field.onMissing) {
          out[label] = field.onMissing(undefined);
          continue;
        }
        if (field.codec.kind !== "option-nullish") return err(schemaError(where, "a missing key"));
      }
      const decoded = field.codec.decodeAt(input[field.wire], where);
      if (decoded._tag === "Failure") return decoded;
      out[label] = field.optional ? some(decoded.success) : decoded.success;
    }
    return ok(out);
  }, (value, path) => {
    if (!isRecord(value)) return err(schemaError(path, `expected an object, got ${describe(value)}`));
    const out: Record<string, unknown> = {};
    for (const [label, field] of fields) {
      const input = value[label];
      if (field.optional) {
        if (!isRecord(input) || input["_tag"] === "None") continue;
        if (input["_tag"] !== "Some") return err(schemaError(at(path, label), "expected an Option"));
        const encoded = field.codec.encodeAt(input["value"], at(path, field.wire));
        if (encoded._tag === "Failure") return encoded;
        out[field.wire] = encoded.success;
      } else {
        const encoded = field.codec.encodeAt(input, at(path, field.wire));
        if (encoded._tag === "Failure") return encoded;
        out[field.wire] = encoded.success;
      }
    }
    return ok(out);
  });
  blocks.set(codec, { name, fields, ignoreExtra });
  return withBlock(codec, fields, ignoreExtra);
};

export const codecBlock = (name: string, fields: BlockFields): Witness => makeBlock(name, fields, false);
export const codecRecord = (name: string, _schema: Witness, fields: BlockFields): Witness => makeBlock(name, fields, false);
export const codecIgnoreExtra = (codec: Codec): Codec => {
  const block = blocks.get(codec);
  return block ? makeBlock(block.name, block.fields, true) : codec;
};

interface StandardResult {
  readonly value?: unknown;
  readonly issues?: ReadonlyArray<{ readonly message: string; readonly path?: ReadonlyArray<string> }>;
}
interface StandardSchema {
  readonly "~standard": {
    readonly version: 1;
    readonly vendor: "flick";
    readonly validate: (value: unknown) => StandardResult | Promise<StandardResult>;
  };
}

const standardResult = (codec: Codec, value: unknown): StandardResult => {
  const result = codecDecodeResult(codec, value);
  return result._tag === "Success" ? { value: result.success }
    : { issues: result.failure._0.map((issue) => ({ message: issue.message,
      path: issue.path === "(root)" ? [] : issue.path.split(".") })) };
};

export const codecStandard = (codec: Codec): StandardSchema => Object.freeze({
  "~standard": Object.freeze({
    version: 1 as const, vendor: "flick" as const,
    validate: (value: unknown): StandardResult => standardResult(codec, value),
  }),
});

export const codecStandardAsync = (codec: Codec): StandardSchema => {
  const owner = currentOwner();
  return Object.freeze({
    "~standard": Object.freeze({
      version: 1 as const, vendor: "flick" as const,
      validate: (value: unknown): Promise<StandardResult> => Promise.resolve(owner && !owner.open
        ? { issues: [{ message: "the scope of this validator has closed" }] }
        : standardResult(codec, value)),
    }),
  });
};

const map = <A, B>(result: { readonly _tag: "Success"; readonly success: A } | { readonly _tag: "Failure"; readonly failure: SchemaError },
  f: (value: A) => B) => result._tag === "Success" ? ok(f(result.success)) : result;
const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const at = (path: string, key: string): string => path ? `${path}.${key}` : key;
const describe = (value: unknown): string =>
  value === null ? "null" : Array.isArray(value) ? "an array" : typeof value === "object" ? "an object" : typeof value;
