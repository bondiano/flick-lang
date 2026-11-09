// Structural descriptions shared by codecs, exporters and runtime bridges.
import type { Witness } from "./schema.ts";

export interface Case { readonly tag: string; readonly fields: ReadonlyArray<Witness> }

export type SchemaNode =
  | { readonly kind: "number" | "string" | "bool" | "int" | "bigint" | "unit" | "never" | "unknown" }
  | { readonly kind: "list" | "option" | "chunk"; readonly item: Witness }
  | { readonly kind: "result"; readonly error: Witness; readonly value: Witness }
  | { readonly kind: "tuple"; readonly items: ReadonlyArray<Witness> }
  | { readonly kind: "record"; readonly fields: ReadonlyArray<readonly [string, Witness]> }
  | { readonly kind: "adt"; readonly cases: ReadonlyArray<Case> }
  | { readonly kind: "lazy"; readonly name: string; readonly get: () => Witness }
  | { readonly kind: "opaque"; readonly reason: string }
  | { readonly kind: "literals"; readonly values: ReadonlyArray<string | number> }
  | { readonly kind: "enum-tags"; readonly pairs: ReadonlyArray<readonly [string, string | number]> }
  | { readonly kind: "refined"; readonly name: string; readonly known: string | undefined; readonly base: Witness }
  | { readonly kind: "transform"; readonly from: Witness }
  | { readonly kind: "duration-millis" }
  | { readonly kind: "entries"; readonly value: Witness; readonly min?: number; readonly max?: number; readonly unsupported: boolean }
  | { readonly kind: "block"; readonly fields: ReadonlyArray<readonly [string, {
    readonly wire: string; readonly codec: Witness; readonly optional: boolean;
    readonly onMissing?: (unit: undefined) => unknown;
    readonly onMissingEffect?: (unit: undefined) => unknown;
  }]>; readonly ignoreExtra: boolean };

const schemaNodes = /*#__PURE__*/ new WeakMap<Witness, SchemaNode>();
export const schemaDescription = (schema: Witness): SchemaNode | undefined => schemaNodes.get(schema);

export const node = (schema: Witness, description: SchemaNode): Witness => {
  schemaNodes.set(schema, description);
  return schema;
};

