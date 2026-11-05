// The combinators `deriving Schema` writes in Flick: a constructor of a
// variant type is given as a value, whose tag the runtime reads.
import { Option, Result } from "effect";

import {
  type AnySchema,
  schemaAdt,
  schemaEnum,
  schemaLazy,
  schemaRecord,
  schemaRefined,
  schemaTuple,
} from "./schemas.ts";

/** The tag of a constructor given as a value: a nullary one is its value, another a function of its fields. */
export const tagOf = (constructor: unknown): string => {
  let value: unknown = constructor;
  // a constructor's stage takes one field, whose value does not change the tag
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a constructor is curried over its fields
  while (typeof value === "function") value = (value as (field: unknown) => unknown)(undefined);
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a constructor's value is tagged data
  return (value as { readonly _tag: string })._tag;
};

/** `Schema.field label schema`. */
export const schemaField = (label: string, schema: AnySchema): readonly [string, AnySchema] => [label, schema];

/** `Schema.unsafe_record fields`, in label order. */
export const schemaRecordOf = (fields: ReadonlyArray<readonly [string, AnySchema]>): AnySchema => schemaRecord(fields);

/** `Schema.item schema`: an item of a tuple is its schema. */
export const schemaItem = (schema: AnySchema): AnySchema => schema;

/** `Schema.unsafe_tuple items`. */
export const schemaTupleOf = (items: ReadonlyArray<AnySchema>): AnySchema => schemaTuple(items);

/** A constructor and the schemas of its fields. */
interface Case {
  readonly tag: string;
  readonly fields: ReadonlyArray<AnySchema>;
}

/** `Schema.unsafe_case c fields`. */
export const schemaCase = (constructor: unknown, fields: ReadonlyArray<AnySchema>): Case => ({
  tag: tagOf(constructor),
  fields,
});

/** `Schema.unsafe_adt name cases`: a constructor's name on the wire is its tag's. */
export const schemaAdtOf = (name: string, cases: ReadonlyArray<Case>): AnySchema =>
  schemaAdt(
    name,
    cases.map((c) => [c.tag, c.tag.split("#")[0] ?? c.tag, c.fields] as const),
  );

/** `Schema.lazy name get`. */
export const schemaLazyOf = (name: string, get: (unit: undefined) => AnySchema): AnySchema =>
  schemaLazy(name, () => get(undefined));

/** `Schema.recursive name of`: `of` gets a reference to the schema it makes. */
export const schemaRecursive = (name: string, of: (self: AnySchema) => AnySchema): AnySchema => {
  // the reference is resolved on use, after `of` has made the schema
  const self = schemaLazy(name, () => made);
  const made = of(self);
  return made;
};

/** `Schema.unsafe_unboxed field`: the values are the field's. */
export const schemaUnboxed = (field: AnySchema): AnySchema => field;

/** `Schema.unsafe_literals base value values`: an enum's values are its literals, on the wire too. */
export const schemaLiterals = (
  _base: AnySchema,
  _value: unknown,
  values: ReadonlyArray<string | number>,
): AnySchema => schemaEnum(values, null);

/** `Schema.unsafe_enum_tags base pairs`: an enum on the wire as a variant type, each constructor's name with its value. */
export const schemaEnumTags = (_base: AnySchema, pairs: ReadonlyArray<readonly [string, string | number]>): AnySchema =>
  schemaEnum(
    pairs.map(([, l]) => l),
    pairs.map(([tag]) => tag),
  );

/** `Schema.unsafe_refined name known base parse`. */
export const schemaRefinedOf = (
  name: string,
  known: Option.Option<string>,
  base: AnySchema,
  parse: (value: unknown) => Result.Result<unknown, unknown>,
): AnySchema => schemaRefined(name, base, (value: never) => Result.isSuccess(parse(value)), Option.getOrNull(known));
