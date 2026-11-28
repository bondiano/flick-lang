// Fixed-width numeric runtime schemas.
import { err, ok } from "../core/data.ts";
import { node } from "./schema_nodes.ts";
import { type Check, type Witness, describe, mismatch, schemaError, witness } from "./schema_core.ts";

const checkBoundedInt = (label: string, min: number, max: number): Check => (v, path) => {
  if (typeof v !== "number") return mismatch(path, `an ${label}`, v);
  return Number.isInteger(v) && v >= min && v <= max
    ? ok(v) : err(schemaError(path, `expected an ${label}, an integer from ${min} to ${max}`));
};

const checkBigIntBounds = (label: string, min: bigint, max: bigint): Check => (v, path) => {
  if (typeof v !== "bigint") return mismatch(path, `an ${label}`, v);
  return v >= min && v <= max
    ? ok(v) : err(schemaError(path, `expected an ${label}, an integer from ${String(min)} to ${String(max)}`));
};

const boundedBigIntWitness = (kind: "int64" | "uint64", label: string, min: bigint, max: bigint): Witness => {
  const valCheck = checkBigIntBounds(label, min, max);
  return node(witness(kind, (v, path) => {
    if (typeof v !== "string" || !/^-?\d+$/.test(v))
      return err(schemaError(path, typeof v === "string" ? "expected a string of digits" : `expected a string of digits, got ${describe(v)}`));
    const n = BigInt(v);
    return n >= min && n <= max
      ? ok(n) : err(schemaError(path, `expected an ${label}, an integer from ${String(min)} to ${String(max)}`));
  }, (v, path) => {
    const checked = valCheck(v, path);
    return checked._tag === "Success" ? ok(String(checked.success)) : checked;
  }, valCheck), { kind });
};

export const schemaInt8: Witness = /*#__PURE__*/ node(witness("int8", checkBoundedInt("Int8", -128, 127), checkBoundedInt("Int8", -128, 127)), { kind: "int8" });
export const schemaInt16: Witness = /*#__PURE__*/ node(witness("int16", checkBoundedInt("Int16", -32768, 32767), checkBoundedInt("Int16", -32768, 32767)), { kind: "int16" });
export const schemaInt64: Witness = /*#__PURE__*/ boundedBigIntWitness("int64", "Int64", -9223372036854775808n, 9223372036854775807n);
export const schemaUInt8: Witness = /*#__PURE__*/ node(witness("uint8", checkBoundedInt("UInt8", 0, 255), checkBoundedInt("UInt8", 0, 255)), { kind: "uint8" });
export const schemaUInt16: Witness = /*#__PURE__*/ node(witness("uint16", checkBoundedInt("UInt16", 0, 65535), checkBoundedInt("UInt16", 0, 65535)), { kind: "uint16" });
export const schemaUInt32: Witness = /*#__PURE__*/ node(witness("uint32", checkBoundedInt("UInt32", 0, 4294967295), checkBoundedInt("UInt32", 0, 4294967295)), { kind: "uint32" });
export const schemaUInt64: Witness = /*#__PURE__*/ boundedBigIntWitness("uint64", "UInt64", 0n, 18446744073709551615n);
export const schemaFloat32: Witness = /*#__PURE__*/ node(witness("float32",
  (v, path) => typeof v !== "number" ? mismatch(path, "a finite Float32", v)
    : Number.isFinite(Math.fround(v)) ? ok(Math.fround(v)) : mismatch(path, "a finite Float32", v),
  (v, path) => typeof v === "number" && !Number.isFinite(v)
    ? err(schemaError(path, `${String(v)} has no JSON form`))
    : typeof v === "number" ? ok(Math.fround(v)) : mismatch(path, "a Float32", v),
  (v, path) => typeof v === "number" ? ok(Math.fround(v)) : mismatch(path, "a Float32", v)), { kind: "float32" });
