// How values cross the PostgreSQL driver. Parameters go as text or
// numbers in the placeholder array, never into the SQL text; results are
// read as text and decoded by the kind the compiler settled, after checking
// the column's type OID, then by the column's codec.
import { BigDecimal, Option, Result } from "effect";

import type { Tagged } from "../data.ts";
import { codecDecodeResult, codecEncodeResult } from "../schema/index.ts";
import type { SchemaError } from "../schema/issues.ts";
import type { AnySchema } from "../schema/schemas.ts";
import { tag } from "../tags.ts";

export const ENCODE_ERROR = tag("Sql.EncodeError.EncodeError");
export const DECODE_ERROR = tag("Sql.DecodeError.DecodeError");
export const DB_ERROR = tag("Sql.DbError.DbError");

/** `Sql.EncodeError`. */
export type EncodeError = Tagged<typeof ENCODE_ERROR, { readonly parameter: string; readonly message: string }>;
/** `Sql.DecodeError`. */
export type DecodeError = Tagged<typeof DECODE_ERROR, { readonly column: string; readonly message: string }>;
/** `Sql.DbError`. */
export type DbError = Tagged<
  typeof DB_ERROR,
  { readonly code: string; readonly message: string; readonly constraint: Option.Option<string> }
>;

export const encodeError = (parameter: string, message: string): EncodeError => ({
  _tag: ENCODE_ERROR,
  _0: { parameter, message },
});
export const decodeError = (column: string, message: string): DecodeError => ({
  _tag: DECODE_ERROR,
  _0: { column, message },
});

/** A driver or server failure as inert data. */
export const dbError = (cause: unknown): DbError => {
  const field = (name: string): string | undefined => {
    const value: unknown = typeof cause === "object" && cause !== null ? Reflect.get(cause, name) : undefined;
    return typeof value === "string" ? value : undefined;
  };
  const constraint = field("constraint");
  return {
    _tag: DB_ERROR,
    _0: {
      code: field("code") ?? "",
      message: field("message") ?? String(cause),
      constraint: constraint === undefined ? Option.none() : Option.some(constraint),
    },
  };
};

/** Type OIDs each kind accepts. */
const OIDS: Readonly<Record<string, ReadonlyArray<number>>> = {
  number: [21, 23, 26, 700, 701],
  bigint: [20],
  numeric: [1700],
  text: [18, 19, 25, 1042, 1043],
  uuid: [2950],
  bool: [16],
  date: [1082],
  timestamp: [1114],
  timestamptz: [1184],
  json: [114, 3802],
};

/** A column of a `sql table`. */
export interface Column {
  readonly name: string;
  readonly kind: string;
  readonly nullable: boolean;
  readonly codec: AnySchema | undefined;
}

/** A value of a kind as the driver takes it. */
export const encodeKind = (kind: string, value: unknown): Result.Result<unknown, string> => {
  switch (kind) {
    case "number":
      return typeof value === "number" && Number.isFinite(value)
        ? Result.succeed(value)
        : Result.fail(`${String(value)} is not a finite number`);
    case "bigint":
      return typeof value === "bigint" ? Result.succeed(value.toString()) : Result.fail("not a BigInt");
    case "numeric":
      return BigDecimal.isBigDecimal(value) ? Result.succeed(BigDecimal.format(value)) : Result.fail("not a BigDecimal");
    case "bool":
      return typeof value === "boolean" ? Result.succeed(value) : Result.fail("not a Bool");
    case "json":
      return Result.succeed(JSON.stringify(value));
    default:
      return typeof value === "string" ? Result.succeed(value) : Result.fail("not a String");
  }
};

/** A value read as text, of a kind. */
export const decodeKind = (kind: string, text: string): Result.Result<unknown, string> => {
  switch (kind) {
    case "number": {
      const n = Number(text);
      return Number.isNaN(n) && text !== "NaN" ? Result.fail(`${text} is not a number`) : Result.succeed(n);
    }
    case "bigint":
      return /^-?\d+$/.test(text) ? Result.succeed(BigInt(text)) : Result.fail(`${text} is not an integer`);
    case "numeric": {
      const d = BigDecimal.fromString(text);
      return Option.isSome(d) ? Result.succeed(d.value) : Result.fail(`${text} is not a decimal`);
    }
    case "bool":
      return text === "t" || text === "f" ? Result.succeed(text === "t") : Result.fail(`${text} is not a boolean`);
    case "json":
      try {
        return Result.succeed(JSON.parse(text));
      } catch {
        return Result.fail("not JSON");
      }
    default:
      return Result.succeed(text);
  }
};

/** Whether the database sent a column of the type the kind reads. */
export const acceptsOid = (kind: string, oid: number): boolean => OIDS[kind]?.includes(oid) ?? false;

/** Encodes `value` through the column's codec, then by its kind. */
export const encodeColumn = (column: Column, value: unknown): Result.Result<unknown, string> => {
  if (column.codec === undefined) return encodeKind(column.kind, value);
  const encoded = codecEncodeResult(column.codec, value);
  return Result.isFailure(encoded) ? Result.fail(message(encoded.failure)) : encodeKind(column.kind, encoded.success);
};

/** Decodes text by the column's kind, then through its codec. */
export const decodeColumn = (column: Column, text: string): Result.Result<unknown, string> => {
  const base = decodeKind(column.kind, text);
  if (column.codec === undefined || Result.isFailure(base)) return base;
  const decoded = codecDecodeResult(column.codec, base.success);
  return Result.isFailure(decoded) ? Result.fail(message(decoded.failure)) : Result.succeed(decoded.success);
};

const message = (failure: SchemaError): string => failure._0.map((i) => `${i.path}: ${i.message}`).join("; ");
