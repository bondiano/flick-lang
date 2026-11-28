// Shared schema core primitives and error constructors for Flick's runtime codecs.
import { type Result, err, ok } from "../core/data.ts";
import { structuralValue } from "./schema_value.ts";
import { type SchemaNode, node } from "./schema_nodes.ts";
import { tag } from "../tags.ts";

export const SCHEMA_ERROR = /*#__PURE__*/ tag("Prelude.SchemaError.SchemaError");

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

export type Check = (value: unknown, path: string) => Result<unknown, SchemaError>;
export type Witness = Codec & { readonly decodeAt: Check; readonly encodeAt: Check; readonly value: Check };

export const at = (path: string, key: string | number): string =>
  typeof key === "number" ? `${path}[${key}]` : path ? `${path}.${key}` : key;

export const mapResult = <A, B>(r: Result<A, SchemaError>, f: (v: A) => B): Result<B, SchemaError> =>
  r._tag === "Success" ? ok(f(r.success)) : r;

export const witness = (kind: string, decode: Check, encode: Check,
  value?: Check,
  description?: SchemaNode): Witness => {
  const schema = {
    kind,
    decode: (input: unknown) => decode(input, ""),
    encode: (input: unknown) => encode(input, ""),
    decodeAt: decode,
    encodeAt: encode,
    value: value ?? ((input: unknown, path: string) => structuralValue(description, input, path, schemaError) ?? mapResult(encode(input, path), () => input)),
  };
  return description ? node(schema, description) : schema;
};

export const describe = (value: unknown): string => {
  if (value === null) return "null";
  if (typeof value === "number" && !Number.isFinite(value)) return String(value);
  if (Array.isArray(value)) return "an array";
  return typeof value === "object" ? "an object" : typeof value;
};

export const mismatch = (path: string, expected: string, input: unknown): Result<never, SchemaError> =>
  err(schemaError(path, `expected ${expected}, got ${describe(input)}`));
