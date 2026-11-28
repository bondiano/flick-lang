// Canonical FLN codec over the runtime's structural schema witnesses.
import { type Result, err, none, ok, some } from "../core/data.ts";
import { tag } from "../tags.ts";
import { type Codec, type SchemaError, type Witness, schemaDescription, schemaError, witness } from "./schema.ts";

const FLN_UNIT = /*#__PURE__*/ tag("Fln.Value.Unit");
const FLN_BOOL = /*#__PURE__*/ tag("Fln.Value.Bool");
const FLN_INT = /*#__PURE__*/ tag("Fln.Value.Int");
const FLN_BIGINT = /*#__PURE__*/ tag("Fln.Value.BigInt");
const FLN_FLOAT = /*#__PURE__*/ tag("Fln.Value.Float");
const FLN_STRING = /*#__PURE__*/ tag("Fln.Value.String");
const FLN_TUPLE = /*#__PURE__*/ tag("Fln.Value.Tuple");
const FLN_LIST = /*#__PURE__*/ tag("Fln.Value.List");
const FLN_RECORD = /*#__PURE__*/ tag("Fln.Value.Record");
const FLN_CONSTRUCTOR = /*#__PURE__*/ tag("Fln.Value.Constructor");

const SUF_I8 = /*#__PURE__*/ tag("Fln.NumSuffix.I8");
const SUF_I16 = /*#__PURE__*/ tag("Fln.NumSuffix.I16");
const SUF_I32 = /*#__PURE__*/ tag("Fln.NumSuffix.I32");
const SUF_I64 = /*#__PURE__*/ tag("Fln.NumSuffix.I64");
const SUF_U8 = /*#__PURE__*/ tag("Fln.NumSuffix.U8");
const SUF_U16 = /*#__PURE__*/ tag("Fln.NumSuffix.U16");
const SUF_U32 = /*#__PURE__*/ tag("Fln.NumSuffix.U32");
const SUF_U64 = /*#__PURE__*/ tag("Fln.NumSuffix.U64");
const SUF_F32 = /*#__PURE__*/ tag("Fln.NumSuffix.F32");

const flnUnit = (): unknown => ({ _tag: FLN_UNIT });
const flnBool = (b: boolean): unknown => ({ _tag: FLN_BOOL, _0: b });
const flnInt = (n: bigint, suf?: string): unknown => ({
  _tag: FLN_INT,
  _0: n,
  _1: suf === undefined ? none : some({ _tag: suf }),
});
const flnBigInt = (n: bigint): unknown => ({ _tag: FLN_BIGINT, _0: n });
const flnFloat = (x: number, suf?: string): unknown => ({
  _tag: FLN_FLOAT,
  _0: x,
  _1: suf === undefined ? none : some({ _tag: suf }),
});
const flnString = (s: string): unknown => ({ _tag: FLN_STRING, _0: s });
const flnTuple = (xs: ReadonlyArray<unknown>): unknown => ({ _tag: FLN_TUPLE, _0: xs });
const flnList = (xs: ReadonlyArray<unknown>): unknown => ({ _tag: FLN_LIST, _0: xs });
const flnRecord = (entries: ReadonlyArray<readonly [string, unknown]>): unknown => ({ _tag: FLN_RECORD, _0: entries });
const flnConstructor = (name: string, args: ReadonlyArray<unknown>): unknown => ({
  _tag: FLN_CONSTRUCTOR,
  _0: [],
  _1: name,
  _2: args,
});

const at = (path: string, key: string | number): string =>
  typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${key}` : key;

const isObj = (v: unknown): v is Readonly<Record<string, unknown>> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

const assign = <A>(record: Record<string, A>, key: string, item: A): void => {
  if (key === "__proto__") Object.defineProperty(record, key, { value: item, writable: true, enumerable: true, configurable: true });
  else record[key] = item;
};

const suffixTagOf = (opt: unknown): string | undefined => {
  if (!isObj(opt) || opt["_tag"] !== "Some") return undefined;
  const inner = opt["value"];
  return isObj(inner) && typeof inner["_tag"] === "string" ? inner["_tag"] : undefined;
};

const describeFln = (v: unknown): string => {
  if (!isObj(v) || typeof v["_tag"] !== "string") return "a non-FLN value";
  switch (v["_tag"]) {
    case FLN_UNIT: return "()";
    case FLN_BOOL: return "Bool";
    case FLN_INT: return "Int";
    case FLN_BIGINT: return "BigInt";
    case FLN_FLOAT: return "Float";
    case FLN_STRING: return "String";
    case FLN_TUPLE: return "Tuple";
    case FLN_LIST: return "List";
    case FLN_RECORD: return "Record";
    case FLN_CONSTRUCTOR: return typeof v["_1"] === "string" ? v["_1"] : "Constructor";
    default: return "a non-FLN value";
  }
};

const fail = (path: string, message: string): Result<never, SchemaError> => err(schemaError(path, message));

const decodeInt = (
  v: unknown,
  path: string,
  label: string,
  wantSuffix: string,
  min: bigint,
  max: bigint,
  asBigInt: boolean,
): Result<unknown, SchemaError> => {
  if (!isObj(v) || v["_tag"] !== FLN_INT || typeof v["_0"] !== "bigint") {
    return fail(path, `expected ${label}, got ${describeFln(v)}`);
  }
  const suf = suffixTagOf(v["_1"]);
  if (suf !== undefined && suf !== wantSuffix) {
    return fail(path, `expected ${label} suffix, got mismatched numeric suffix`);
  }
  const n = v["_0"];
  if (n < min || n > max) {
    return fail(path, `expected ${label}, an integer from ${String(min)} to ${String(max)}`);
  }
  return ok(asBigInt ? n : Number(n));
};

const decodeNode = (schema: Witness, v: unknown, path: string): Result<unknown, SchemaError> => {
  const desc = schemaDescription(schema);
  if (desc === undefined) return fail(path, "schema has no canonical FLN form");
  switch (desc.kind) {
    case "unit":
      return isObj(v) && v["_tag"] === FLN_UNIT ? ok(undefined) : fail(path, `expected (), got ${describeFln(v)}`);
    case "bool":
      return isObj(v) && v["_tag"] === FLN_BOOL && typeof v["_0"] === "boolean"
        ? ok(v["_0"]) : fail(path, `expected a boolean, got ${describeFln(v)}`);
    case "string":
      return isObj(v) && v["_tag"] === FLN_STRING && typeof v["_0"] === "string"
        ? ok(v["_0"]) : fail(path, `expected a string, got ${describeFln(v)}`);
    case "int": return decodeInt(v, path, "an Int", SUF_I32, -2147483648n, 2147483647n, false);
    case "int8": return decodeInt(v, path, "an Int8", SUF_I8, -128n, 127n, false);
    case "int16": return decodeInt(v, path, "an Int16", SUF_I16, -32768n, 32767n, false);
    case "int64": return decodeInt(v, path, "an Int64", SUF_I64, -9223372036854775808n, 9223372036854775807n, true);
    case "uint8": return decodeInt(v, path, "a UInt8", SUF_U8, 0n, 255n, false);
    case "uint16": return decodeInt(v, path, "a UInt16", SUF_U16, 0n, 65535n, false);
    case "uint32": return decodeInt(v, path, "a UInt32", SUF_U32, 0n, 4294967295n, false);
    case "uint64": return decodeInt(v, path, "a UInt64", SUF_U64, 0n, 18446744073709551615n, true);
    case "bigint":
      return isObj(v) && v["_tag"] === FLN_BIGINT && typeof v["_0"] === "bigint"
        ? ok(v["_0"]) : fail(path, `expected a BigInt, got ${describeFln(v)}`);
    case "number": {
      if (isObj(v) && v["_tag"] === FLN_FLOAT && typeof v["_0"] === "number" && suffixTagOf(v["_1"]) === undefined) {
        return Number.isFinite(v["_0"]) ? ok(v["_0"]) : fail(path, "expected a finite number");
      }
      if (isObj(v) && v["_tag"] === FLN_INT && typeof v["_0"] === "bigint" && suffixTagOf(v["_1"]) === undefined) {
        const n = Number(v["_0"]);
        return Number.isFinite(n) ? ok(n) : fail(path, "expected a finite number");
      }
      return fail(path, `expected a finite number, got ${describeFln(v)}`);
    }
    case "float32": {
      if (isObj(v) && v["_tag"] === FLN_FLOAT && typeof v["_0"] === "number") {
        const suf = suffixTagOf(v["_1"]);
        if (suf === undefined || suf === SUF_F32) {
          const f = Math.fround(v["_0"]);
          return Number.isFinite(f) ? ok(f) : fail(path, "expected a finite Float32");
        }
      }
      if (isObj(v) && v["_tag"] === FLN_INT && typeof v["_0"] === "bigint" && suffixTagOf(v["_1"]) === undefined) {
        const f = Math.fround(Number(v["_0"]));
        return Number.isFinite(f) ? ok(f) : fail(path, "expected a finite Float32");
      }
      return fail(path, `expected a Float32, got ${describeFln(v)}`);
    }
    case "never": return fail(path, `expected nothing (Never), got ${describeFln(v)}`);
    case "unknown": return ok(v);
    case "opaque": return fail(path, desc.reason);
    case "list": {
      if (!isObj(v) || v["_tag"] !== FLN_LIST || !Array.isArray(v["_0"])) {
        return fail(path, `expected a list, got ${describeFln(v)}`);
      }
      const out: unknown[] = [];
      for (let i = 0; i < v["_0"].length; i++) {
        const r = decodeNode(desc.item, v["_0"][i], at(path, i));
        if (r._tag === "Failure") return r;
        out.push(r.success);
      }
      return ok(out);
    }
    case "tuple": {
      if (!isObj(v) || v["_tag"] !== FLN_TUPLE || !Array.isArray(v["_0"])) {
        return fail(path, `expected a tuple of ${desc.items.length}, got ${describeFln(v)}`);
      }
      const items = v["_0"];
      const out: unknown[] = [];
      for (const [i, item] of desc.items.entries()) {
        if (i >= items.length) return fail(at(path, i), "a missing tuple item");
        const r = decodeNode(item, items[i], at(path, i));
        if (r._tag === "Failure") return r;
        out.push(r.success);
      }
      if (items.length > desc.items.length) return fail(at(path, desc.items.length), "an extra tuple item");
      return ok(out);
    }
    case "option": {
      if (!isObj(v) || v["_tag"] !== FLN_CONSTRUCTOR || typeof v["_1"] !== "string" || !Array.isArray(v["_2"])) {
        return fail(path, `expected None or Some, got ${describeFln(v)}`);
      }
      if (v["_1"] === "None" && v["_2"].length === 0) return ok(none);
      if (v["_1"] === "Some" && v["_2"].length === 1) {
        const r = decodeNode(desc.item, v["_2"][0], path);
        return r._tag === "Success" ? ok(some(r.success)) : r;
      }
      return fail(path, `expected None or Some, got ${v["_1"]}`);
    }
    case "result": {
      if (!isObj(v) || v["_tag"] !== FLN_CONSTRUCTOR || typeof v["_1"] !== "string" || !Array.isArray(v["_2"])) {
        return fail(path, `expected Ok or Err, got ${describeFln(v)}`);
      }
      if (v["_1"] === "Ok" && v["_2"].length === 1) {
        const r = decodeNode(desc.value, v["_2"][0], at(path, "ok"));
        return r._tag === "Success" ? ok(ok(r.success)) : r;
      }
      if (v["_1"] === "Err" && v["_2"].length === 1) {
        const r = decodeNode(desc.error, v["_2"][0], at(path, "err"));
        return r._tag === "Success" ? ok(err(r.success)) : r;
      }
      return fail(path, `expected Ok or Err, got ${v["_1"]}`);
    }
    case "record": {
      if (!isObj(v) || v["_tag"] !== FLN_RECORD || !Array.isArray(v["_0"])) {
        return fail(path, `expected a record, got ${describeFln(v)}`);
      }
      const byKey = new Map<string, unknown>();
      for (const pair of v["_0"]) {
        if (Array.isArray(pair) && typeof pair[0] === "string") byKey.set(pair[0], pair[1]);
      }
      const out: Record<string, unknown> = {};
      for (const [name, fieldSchema] of desc.fields) {
        if (!byKey.has(name)) return fail(at(path, name), "a missing key");
        const r = decodeNode(fieldSchema, byKey.get(name), at(path, name));
        if (r._tag === "Failure") return r;
        assign(out, name, r.success);
      }
      for (const name of byKey.keys()) {
        if (!desc.fields.some(([label]) => label === name)) return fail(at(path, name), "an unknown key");
      }
      return ok(out);
    }
    case "adt": {
      if (!isObj(v) || v["_tag"] !== FLN_CONSTRUCTOR || typeof v["_1"] !== "string" || !Array.isArray(v["_2"])) {
        return fail(path, `expected a constructor, got ${describeFln(v)}`);
      }
      const name = v["_1"];
      const args = v["_2"];
      const selected = desc.cases.find((c) => c.tag.split("#")[0] === name);
      if (!selected) {
        return fail(path, `expected one of ${desc.cases.map((c) => c.tag.split("#")[0]).join(", ")}`);
      }
      if (args.length !== selected.fields.length) {
        return fail(path, `constructor ${name} expects ${selected.fields.length} argument(s), got ${args.length}`);
      }
      const out: Record<string, unknown> = { _tag: selected.tag };
      for (let i = 0; i < selected.fields.length; i++) {
        const field = selected.fields[i];
        if (!field) continue;
        const r = decodeNode(field, args[i], at(path, `_${i}`));
        if (r._tag === "Failure") return r;
        out[`_${i}`] = r.success;
      }
      return ok(out);
    }
    case "lazy": return decodeNode(desc.get(), v, path);
    case "literals": {
      let raw: string | number | undefined;
      if (isObj(v) && v["_tag"] === FLN_STRING && typeof v["_0"] === "string") raw = v["_0"];
      else if (isObj(v) && v["_tag"] === FLN_INT && typeof v["_0"] === "bigint") raw = Number(v["_0"]);
      if (raw !== undefined && desc.values.includes(raw)) return ok(raw);
      return fail(path, `expected one of ${desc.values.map((x) => JSON.stringify(x)).join(", ")}`);
    }
    case "enum-tags": {
      if (!isObj(v) || v["_tag"] !== FLN_CONSTRUCTOR || typeof v["_1"] !== "string" || !Array.isArray(v["_2"]) || v["_2"].length !== 0) {
        return fail(path, `expected one of ${desc.pairs.map(([n]) => n).join(", ")}`);
      }
      const hit = desc.pairs.find(([n]) => n === v["_1"]);
      return hit ? ok(hit[1]) : fail(path, `expected one of ${desc.pairs.map(([n]) => n).join(", ")}`);
    }
    case "refined": {
      const r = decodeNode(desc.base, v, path);
      if (r._tag === "Failure") return r;
      const checked = schema.value(r.success, path);
      return checked._tag === "Failure" ? checked : ok(r.success);
    }
    default: return fail(path, "schema has no canonical FLN form");
  }
};

const encodeNode = (schema: Witness, v: unknown, path: string): Result<unknown, SchemaError> => {
  const desc = schemaDescription(schema);
  if (desc === undefined) return fail(path, "schema has no canonical FLN form");
  switch (desc.kind) {
    case "unit": return v === undefined ? ok(flnUnit()) : fail(path, "expected ()");
    case "bool": return typeof v === "boolean" ? ok(flnBool(v)) : fail(path, "expected a boolean");
    case "string": return typeof v === "string" ? ok(flnString(v)) : fail(path, "expected a string");
    case "int": return typeof v === "number" && Number.isInteger(v) ? ok(flnInt(BigInt(v))) : fail(path, "expected an Int");
    case "int8": return typeof v === "number" ? ok(flnInt(BigInt(v), SUF_I8)) : fail(path, "expected an Int8");
    case "int16": return typeof v === "number" ? ok(flnInt(BigInt(v), SUF_I16)) : fail(path, "expected an Int16");
    case "int64": return typeof v === "bigint" ? ok(flnInt(v, SUF_I64)) : fail(path, "expected an Int64");
    case "uint8": return typeof v === "number" ? ok(flnInt(BigInt(v), SUF_U8)) : fail(path, "expected a UInt8");
    case "uint16": return typeof v === "number" ? ok(flnInt(BigInt(v), SUF_U16)) : fail(path, "expected a UInt16");
    case "uint32": return typeof v === "number" ? ok(flnInt(BigInt(v), SUF_U32)) : fail(path, "expected a UInt32");
    case "uint64": return typeof v === "bigint" ? ok(flnInt(v, SUF_U64)) : fail(path, "expected a UInt64");
    case "bigint": return typeof v === "bigint" ? ok(flnBigInt(v)) : fail(path, "expected a BigInt");
    case "number":
      if (typeof v !== "number") return fail(path, "expected a number");
      if (!Number.isFinite(v)) return fail(path, `${String(v)} has no FLN form`);
      return ok(flnFloat(v));
    case "float32": {
      if (typeof v !== "number") return fail(path, "expected a Float32");
      const f = Math.fround(v);
      if (!Number.isFinite(f)) return fail(path, `${String(f)} has no FLN form`);
      return ok(flnFloat(f, SUF_F32));
    }
    case "never": return fail(path, "expected nothing (Never)");
    case "unknown": return ok(v);
    case "opaque": return fail(path, desc.reason);
    case "list": {
      if (!Array.isArray(v)) return fail(path, "expected a list");
      const out: unknown[] = [];
      for (let i = 0; i < v.length; i++) {
        const r = encodeNode(desc.item, v[i], at(path, i));
        if (r._tag === "Failure") return r;
        out.push(r.success);
      }
      return ok(flnList(out));
    }
    case "tuple": {
      if (!Array.isArray(v)) return fail(path, `expected a tuple of ${desc.items.length}`);
      const out: unknown[] = [];
      for (const [i, item] of desc.items.entries()) {
        const r = encodeNode(item, v[i], at(path, i));
        if (r._tag === "Failure") return r;
        out.push(r.success);
      }
      return ok(flnTuple(out));
    }
    case "option": {
      if (!isObj(v)) return fail(path, "expected an Option");
      if (v["_tag"] === "None") return ok(flnConstructor("None", []));
      if (v["_tag"] === "Some") {
        const r = encodeNode(desc.item, v["value"], path);
        return r._tag === "Success" ? ok(flnConstructor("Some", [r.success])) : r;
      }
      return fail(path, "expected an Option");
    }
    case "result": {
      if (!isObj(v)) return fail(path, "expected a Result");
      if (v["_tag"] === "Success") {
        const r = encodeNode(desc.value, v["success"], at(path, "ok"));
        return r._tag === "Success" ? ok(flnConstructor("Ok", [r.success])) : r;
      }
      if (v["_tag"] === "Failure") {
        const r = encodeNode(desc.error, v["failure"], at(path, "err"));
        return r._tag === "Success" ? ok(flnConstructor("Err", [r.success])) : r;
      }
      return fail(path, "expected a Result");
    }
    case "record": {
      if (!isObj(v)) return fail(path, "expected a record");
      const out: Array<readonly [string, unknown]> = [];
      for (const [name, fieldSchema] of desc.fields) {
        if (!Object.hasOwn(v, name)) return fail(at(path, name), "a missing key");
        const r = encodeNode(fieldSchema, v[name], at(path, name));
        if (r._tag === "Failure") return r;
        out.push([name, r.success]);
      }
      return ok(flnRecord(out));
    }
    case "adt": {
      if (!isObj(v)) return fail(path, "expected a variant");
      const selected = desc.cases.find((c) => c.tag === v["_tag"]);
      if (!selected) return fail(path, "expected a variant");
      const name = selected.tag.split("#")[0] ?? selected.tag;
      const args: unknown[] = [];
      for (let i = 0; i < selected.fields.length; i++) {
        const field = selected.fields[i];
        if (!field) continue;
        const r = encodeNode(field, v[`_${i}`], at(path, `_${i}`));
        if (r._tag === "Failure") return r;
        args.push(r.success);
      }
      return ok(flnConstructor(name, args));
    }
    case "lazy": return encodeNode(desc.get(), v, path);
    case "literals":
      if (typeof v === "string") return ok(flnString(v));
      if (typeof v === "number") return ok(flnInt(BigInt(v)));
      return fail(path, "expected an enum literal");
    case "enum-tags": {
      const hit = desc.pairs.find(([, val]) => val === v);
      return hit ? ok(flnConstructor(hit[0], [])) : fail(path, "expected an enum value");
    }
    case "refined": {
      const checked = schema.value(v, path);
      return checked._tag === "Failure" ? checked : encodeNode(desc.base, v, path);
    }
    default: return fail(path, "schema has no canonical FLN form");
  }
};

export const schemaFlnCodec = (schema: Witness): Codec =>
  witness(
    "fln",
    (input, path) => decodeNode(schema, input, path),
    (value, path) => encodeNode(schema, value, path),
    schema.value,
  );
