// Pure codec values on Flick's own runtime. The JSON codec accepts only
// finite numbers: parsing 1e400 yields Infinity.
import { type Result, err, none, ok, some } from "../core/data.ts";
import { type Case, type SchemaNode, node } from "./schema_nodes.ts";
export { schemaDescription } from "./schema_nodes.ts";
import { tag } from "../tags.ts";

const SCHEMA_ERROR = /*#__PURE__*/ tag("Prelude.SchemaError.SchemaError");

export interface SchemaIssue {
  readonly path: string;
  readonly message: string;
}

export interface SchemaError {
  readonly _tag: string;
  readonly _0: ReadonlyArray<SchemaIssue>;
}

export interface ExportError {
  readonly _tag: string;
  readonly _0: string;
}

export const schemaError = (path: string, message: string): SchemaError => ({
  _tag: SCHEMA_ERROR,
  _0: [{ path: path || "(root)", message }],
});

export interface Codec<A = unknown> {
  readonly kind: string;
  readonly decode: (input: unknown) => Result<A, SchemaError>;
  readonly encode: (value: A) => Result<unknown, SchemaError>;
}

type Check = (value: unknown, path: string) => Result<unknown, SchemaError>;
export type Witness = Codec & { readonly decodeAt: Check; readonly encodeAt: Check; readonly value: Check };

const at = (path: string, key: string | number): string =>
  typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${key}` : key;

export const witness = (kind: string, decode: Check, encode: Check,
  value: Check = (input, path) => map(encode(input, path), () => input),
  description?: SchemaNode): Witness => {
  const schema = {
    kind,
    decode: (input: unknown) => decode(input, ""),
    encode: (input: unknown) => encode(input, ""),
    decodeAt: decode,
    encodeAt: encode,
    value,
  };
  return description ? node(schema, description) : schema;
};

const mismatch = (path: string, expected: string, input: unknown): Result<never, SchemaError> =>
  err(schemaError(path, `expected ${expected}, got ${describe(input)}`));


const primitive = (kind: string, expected: string, valid: (v: unknown) => boolean): Witness => {
  const check: Check = (input, path) => valid(input) ? ok(input) : mismatch(path, expected, input);
  return witness(kind, check, check);
};

export const schemaNumber = node(witness("number",
  (v, path) => typeof v !== "number" ? mismatch(path, "a finite number", v)
    : Number.isFinite(v) ? ok(v) : mismatch(path, "a finite number", v),
  (v, path) => typeof v === "number" && !Number.isFinite(v)
    ? err(schemaError(path, `${String(v)} has no JSON form`))
    : typeof v === "number" ? ok(v) : mismatch(path, "a number", v),
  (v, path) => typeof v === "number" ? ok(v) : mismatch(path, "a number", v)), { kind: "number" });
export const schemaString = node(primitive("string", "a string", (v) => typeof v === "string"), { kind: "string" });
export const schemaBool = node(primitive("bool", "a boolean", (v) => typeof v === "boolean"), { kind: "bool" });
const checkInt: Check = (v, path) => {
    if (typeof v !== "number") return mismatch(path, "an Int", v);
    return Number.isInteger(v) && v >= -2147483648 && v <= 2147483647
      ? ok(v) : err(schemaError(path, "expected an Int, an integer from -2147483648 to 2147483647"));
};
export const schemaInt: Witness = node(witness("int", checkInt, checkInt), { kind: "int" });
export const schemaBigint: Witness = node(witness("bigint", (v, path) => {
  if (typeof v !== "string" || !/^-?\d+$/.test(v))
    return err(schemaError(path, typeof v === "string" ? "expected a string of digits" : `expected a string of digits, got ${describe(v)}`));
  return ok(BigInt(v));
}, (v, path) => typeof v === "bigint" ? ok(String(v)) : mismatch(path, "a BigInt", v),
(v, path) => typeof v === "bigint" ? ok(v) : mismatch(path, "a BigInt", v)), { kind: "bigint" });
export const schemaUnit = node(witness("unit", (v, path) => v === null ? ok(undefined) : mismatch(path, "null", v),
  (v, path) => v === undefined ? ok(null) : mismatch(path, "()", v),
  (v, path) => v === undefined ? ok(v) : mismatch(path, "()", v)), { kind: "unit" });
export const schemaNever = node(primitive("never", "nothing (Never)", () => false), { kind: "never" });
export const schemaUnknown = node(witness("unknown", (v) => ok(v), (v) => ok(v)), { kind: "unknown" });

export const schemaList = (item: Witness): Witness => witness("list", (input, path) => {
  if (!Array.isArray(input)) return mismatch(path, "an array", input);
  const out: unknown[] = [];
  for (let i = 0; i < input.length; i++) {
    const result = item.decodeAt(input[i], at(path, i));
    if (result._tag === "Failure") return result;
    out.push(result.success);
  }
  return ok(out);
}, (input, path) => {
  if (!Array.isArray(input)) return mismatch(path, "an array", input);
  const out: unknown[] = [];
  for (let i = 0; i < input.length; i++) {
    const result = item.encodeAt(input[i], at(path, i));
    if (result._tag === "Failure") return result;
    out.push(result.success);
  }
  return ok(out);
}, undefined, { kind: "list", item });

export const schemaOption = (item: Witness): Witness => witness("option", (v, path) => {
  if (v === null) return ok(none);
  const result = item.decodeAt(v, path);
  return result._tag === "Success" ? ok(some(result.success)) : mismatch(path, "Union", v);
},
  (v, path) => {
    if (typeof v !== "object" || v === null) return mismatch(path, "an Option", v);
    const option = v as { _tag?: string; value?: unknown };
    return option._tag === "None" ? ok(null) : option._tag === "Some"
      ? item.encodeAt(option.value, path) : mismatch(path, "an Option", v);
  }, undefined, { kind: "option", item });

export const schemaResult = (error: Witness, value: Witness): Witness => witness("result", (v, path) => {
  if (!isRecord(v))
    return mismatch(path, "an object with `ok` or `err`", v);
  const hasOk = Object.hasOwn(v, "ok");
  const hasErr = Object.hasOwn(v, "err");
  if (!hasOk && !hasErr) return err({ _tag: SCHEMA_ERROR, _0: [
    { path: at(path, "ok"), message: "a missing key" },
    { path: at(path, "err"), message: "a missing key" },
  ] });
  if (hasOk && !hasErr) {
    const decoded = value.decodeAt(v["ok"], at(path, "ok"));
    return decoded._tag === "Success" ? ok(ok(decoded.success)) : err({ _tag: SCHEMA_ERROR, _0: [
      ...decoded.failure._0,
      { path: at(path, "ok"), message: "an unknown key" },
    ] });
  }
  if (hasErr && !hasOk) {
    const decoded = error.decodeAt(v["err"], at(path, "err"));
    return decoded._tag === "Success" ? ok(err(decoded.success)) : err({ _tag: SCHEMA_ERROR, _0: [
      { path: at(path, "err"), message: "an unknown key" },
      ...decoded.failure._0,
    ] });
  }
  return mismatch(path, "an object with `ok` or `err`", v);
}, (v, path) => {
  if (!isRecord(v)) return mismatch(path, "a Result", v);
  return v["_tag"] === "Success" ? map(value.encodeAt(v["success"], at(path, "ok")), (success) => ({ ok: success }))
    : v["_tag"] === "Failure" ? map(error.encodeAt(v["failure"], at(path, "err")), (failure) => ({ err: failure }))
      : mismatch(path, "a Result", v);
}, undefined, { kind: "result", error, value });

export const schemaField = (label: string, schema: Witness): readonly [string, Witness] => [label, schema];
export const schemaItem = (schema: Witness): Witness => schema;
export const schemaTupleOf = (items: ReadonlyArray<Witness>): Witness => witness("tuple", (v, path) => {
  if (!Array.isArray(v)) return mismatch(path, `a tuple of ${items.length}`, v);
  const out: unknown[] = [];
  for (const [i, item] of items.entries()) {
    if (i >= v.length) return err(schemaError(at(path, i), "a missing key"));
    const r = item.decodeAt(v[i], at(path, i));
    if (r._tag === "Failure") return r;
    out.push(r.success);
  }
  if (v.length > items.length) return err(schemaError(at(path, items.length), "an unknown key"));
  return ok(out);
}, (v, path) => {
  if (!Array.isArray(v)) return mismatch(path, `a tuple of ${items.length}`, v);
  const out: unknown[] = [];
  for (const [i, item] of items.entries()) {
    const r = item.encodeAt(v[i], at(path, i));
    if (r._tag === "Failure") return r;
    out.push(r.success);
  }
  return ok(out);
}, undefined, { kind: "tuple", items });
export const schemaRecordOf = (fields: ReadonlyArray<readonly [string, Witness]>): Witness => witness("record", (v, path) => {
  if (!isRecord(v)) return mismatch(path, "an object", v);
  const out: Record<string, unknown> = {};
  for (const [name, item] of fields) {
    if (!Object.hasOwn(v, name)) return err(schemaError(at(path, name), "a missing key"));
    const r = item.decodeAt(v[name], at(path, name));
    if (r._tag === "Failure") return r;
    out[name] = r.success;
  }
  for (const name of Object.keys(v)) if (!fields.some(([label]) => label === name))
    return err(schemaError(at(path, name), "an unknown key"));
  return ok(out);
}, (v, path) => {
  if (!isRecord(v)) return mismatch(path, "an object", v);
  const out: Record<string, unknown> = {};
  for (const [name, item] of fields) {
    if (!Object.hasOwn(v, name)) return err(schemaError(at(path, name), "a missing key"));
    const r = item.encodeAt(v[name], at(path, name));
    if (r._tag === "Failure") return r;
    out[name] = r.success;
  }
  return ok(out);
}, undefined, { kind: "record", fields });

export const schemaCase = (constructor: unknown, fields: ReadonlyArray<Witness>): Case => {
  let current = constructor;
  // Compiler-generated constructor stages accept one positional field.
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- compiler ABI
  while (typeof current === "function") current = (current as (v: unknown) => unknown)(undefined);
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- compiler ABI
  return { tag: (current as { _tag: string })._tag, fields };
};
export const schemaAdtOf = (name: string, cases: ReadonlyArray<Case>): Witness => witness(name, (v, path) => {
  if (!isRecord(v)) return mismatch(path, "an object with a `_tag`", v);
  const selected = cases.find((c) => c.tag.split("#")[0] === v["_tag"]);
  if (!selected) return err(schemaError(at(path, "_tag"), `expected one of ${cases.map((c) => c.tag.split("#")[0]).join(", ")}`));
  const out: Record<string, unknown> = { _tag: selected.tag };
  for (let i = 0; i < selected.fields.length; i++) {
    const key = `_${i}`;
    if (!Object.hasOwn(v, key)) return err(schemaError(at(path, key), "a missing key"));
    const field = selected.fields[i];
    if (!field) throw new Error("schema case field missing");
    const r = field.decodeAt(v[key], at(path, key));
    if (r._tag === "Failure") return r;
    out[key] = r.success;
  }
  return ok(out);
}, (v, path) => {
  if (!isRecord(v)) return mismatch(path, `a ${name}`, v);
  const selected = cases.find((c) => c.tag === v["_tag"]);
  if (!selected) return mismatch(path, `a ${name}`, v);
  const out: Record<string, unknown> = { _tag: selected.tag.split("#")[0] };
  for (let i = 0; i < selected.fields.length; i++) {
    const key = `_${i}`;
    const field = selected.fields[i];
    if (!field) throw new Error("schema case field missing");
    const r = field.encodeAt(v[key], at(path, key));
    if (r._tag === "Failure") return r;
    out[key] = r.success;
  }
  return ok(out);
}, undefined, { kind: "adt", cases });
export const schemaLazyOf = (name: string, get: (unit: undefined) => Witness): Witness => {
  let target: Witness | undefined;
  const resolve = (): Witness => target ??= get(undefined);
  return witness(name, (v, path) => resolve().decodeAt(v, path), (v, path) => resolve().encodeAt(v, path),
    undefined, { kind: "lazy", name, get: resolve });
};
export const schemaRecursive = (name: string, of: (self: Witness) => Witness): Witness => {
  const self = schemaLazyOf(name, () => {
    return made;
  });
  const made = of(self);
  return made;
};
export const schemaUnboxed = (schema: Witness): Witness => schema;
export const schemaOpaque = (name: string, reason: string): Witness => witness(name,
  (_v, path) => err(schemaError(path, reason)), (_v, path) => err(schemaError(path, reason)), (v) => ok(v),
  { kind: "opaque", reason });
export const schemaRedacted = (_item: Witness): Witness => schemaOpaque("redacted", "a Redacted value has no JSON form without an explicit policy");

export const schemaLiterals = (_base: Witness, _value: unknown, values: ReadonlyArray<string | number>): Witness => {
  const check: Check = (v, path) => (typeof v === "string" || typeof v === "number") && values.includes(v)
    ? ok(v) : err(schemaError(path, `expected one of ${values.map((x) => JSON.stringify(x)).join(", ")}`));
  return witness("literals", check, check, undefined, { kind: "literals", values });
};
export const schemaEnumTags = (_base: Witness, pairs: ReadonlyArray<readonly [string, string | number]>): Witness =>
  witness("enum-tags", (v, path) => {
    if (!isRecord(v)) return mismatch(path, "an object with a `_tag`", v);
    const match = pairs.find(([name]) => name === v["_tag"]);
    return match ? ok(match[1]) : err(schemaError(at(path, "_tag"), `expected one of ${pairs.map(([name]) => name).join(", ")}`));
  }, (v, path) => {
    const match = pairs.find(([, value]) => value === v);
    return match ? ok({ _tag: match[0] }) : mismatch(path, "an enum value", v);
  }, undefined, { kind: "enum-tags", pairs });
export const schemaRefinedOf = (name: string, _known: unknown, base: Witness,
  parse: (v: unknown) => Result<unknown, SchemaError>): Witness => {
  const check: Check = (v, path) => {
    const valid = base.value(v, path);
    if (valid._tag === "Failure") return valid;
    const parsed = parse(v);
    return parsed._tag === "Success" ? ok(v) : err(schemaError(path, `not a valid ${name}`));
  };
  return witness(name, (v, path) => {
    const decoded = base.decodeAt(v, path);
    return decoded._tag === "Failure" ? decoded : check(decoded.success, path);
  }, (v, path) => {
    const valid = check(v, path);
    return valid._tag === "Failure" ? valid : base.encodeAt(v, path);
  }, check, { kind: "refined", name, known: isRecord(_known) && _known["_tag"] === "Some"
    && typeof _known["value"] === "string" ? _known["value"] : undefined, base });
};
export const refinedParse = (name: string, check: (value: unknown) => boolean) =>
  (value: unknown): Result<unknown, SchemaError> =>
    check(value) ? ok(value) : err(schemaError("", `not a valid ${name}`));
export const checkEmail = (value: string): boolean => /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value);
export const checkNonEmpty = (value: string): boolean => value.length > 0;
export const checkPositive = (value: number): boolean => value > 0;
export const schemaJsonCodec = (schema: Witness): Codec => schema;
export const withWire = (codec: Witness, from: Witness): Witness => node(codec, { kind: "transform", from });
export const withBlock = (codec: Witness, fields: ReadonlyArray<readonly [string, {
  readonly wire: string; readonly codec: Witness; readonly optional: boolean;
  readonly onMissing?: (unit: undefined) => unknown;
  readonly onMissingEffect?: (unit: undefined) => unknown;
}]>, ignoreExtra: boolean): Witness => node(codec, { kind: "block", fields, ignoreExtra });
export const codecTransformPure = (
  decode: (wire: unknown) => unknown,
  encode: (value: unknown) => unknown,
  from: Witness,
): Witness => witness("transform",
  (input, path) => map(from.decodeAt(input, path), decode),
  (value, path) => from.encodeAt(encode(value), path),
  undefined, { kind: "transform", from });
export const schemaValidate = (schema: Witness, value: unknown): Result<unknown, SchemaError> => schema.value(value, "");

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);
const map = <A, B>(r: Result<A, SchemaError>, f: (v: A) => B): Result<B, SchemaError> =>
  r._tag === "Success" ? ok(f(r.success)) : r;

const inertProblem = (input: unknown): SchemaIssue | undefined => {
  const active = new Set<object>();
  const walk = (value: unknown, path: string): SchemaIssue | undefined => {
    if (typeof value !== "object" || value === null) return undefined;
    if (active.has(value)) return { path, message: "a cycle in the input" };
    active.add(value);
    for (const key of Object.keys(value)) {
      const next = Array.isArray(value) ? at(path, Number(key)) : at(path, key);
      const property = Object.getOwnPropertyDescriptor(value, key);
      if (!property || !("value" in property)) return { path: next, message: "a getter, not data" };
      const problem = walk(property.value, next);
      if (problem) return problem;
    }
    active.delete(value);
    return undefined;
  };
  return walk(input, "");
};

const describe = (value: unknown): string => {
  if (value === null) return "null";
  if (typeof value === "number" && !Number.isFinite(value)) return String(value);
  if (Array.isArray(value)) return "an array";
  return typeof value === "object" ? "an object" : typeof value;
};

const isJson = (value: unknown, ancestors: Set<object> = new Set()): boolean => {
  if (value === null || typeof value === "string" || typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  if (typeof value !== "object" || ancestors.has(value)) return false;
  ancestors.add(value);
  const valid = (Array.isArray(value) || Object.getPrototypeOf(value) === Object.prototype)
    && Object.keys(value).every((key) => {
      const property = Object.getOwnPropertyDescriptor(value, key);
      return property !== undefined && "value" in property && isJson(property.value, ancestors);
    });
  ancestors.delete(value);
  return valid;
};

const jsonResult = (input: unknown): Result<unknown, SchemaError> =>
  isJson(input) ? ok(input) : err(schemaError("", `expected JSON value, got ${describe(input)}`));

/** `Json.json`: the identity codec of any JSON value. */
export const jsonCodec: Codec = /*#__PURE__*/ Object.freeze({ kind: "json", decode: jsonResult, encode: jsonResult });

export const codecDecodeResult = <A>(codec: Codec<A>, input: unknown): Result<A, SchemaError> => {
  const problem = inertProblem(input);
  return problem ? err(schemaError(problem.path, problem.message)) : codec.decode(input);
};
export const codecEncodeResult = <A>(codec: Codec<A>, value: A): Result<unknown, SchemaError> => codec.encode(value);
