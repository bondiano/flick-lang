// A property-test generator decorates a witness without changing its codec.
import type { Gen } from "../core/testing.ts";

import { schemaDescription, type Witness, witness } from "./schema.ts";

const schemaGenerators = /*#__PURE__*/ new WeakMap<Witness, Gen<unknown>>();
export const schemaGenerator = (schema: Witness): Gen<unknown> | undefined => schemaGenerators.get(schema);

export const schemaWithGenerator = <A>(gen: Gen<A>, schema: Witness): Witness => {
  const wrapped = witness(schema.kind, schema.decodeAt, schema.encodeAt, schema.value, schemaDescription(schema));
  schemaGenerators.set(wrapped, gen);
  return wrapped;
};
