// Effect's `Struct` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Combiner, type Reducer, Struct } from "effect";

export * from "effect/Struct";

const canonicalFields = <T extends object>(fields: T): T => {
  const out: Record<string, unknown> = {};
  for (const key of Object.keys(fields).toSorted()) {
    Object.defineProperty(out, key, {
      value: Reflect.get(fields, key),
      enumerable: true,
      writable: true,
      configurable: true,
    });
  }
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- exact own-key permutation of `fields`
  return out as T;
};

/** `Combiner.struct`: the `Record.Map` dictionary is erased; Flick records are exact, so no `omitKeyWhen`. */
export const flickMakeCombiner = <A>(_map: unknown, combiners: { readonly [K in keyof A]: Combiner.Combiner<A[K]> }) =>
  Struct.makeCombiner<A>(canonicalFields(combiners));

/** `Reducer.struct`: as `flickMakeCombiner`. */
export const flickMakeReducer = <A>(_map: unknown, reducers: { readonly [K in keyof A]: Reducer.Reducer<A[K]> }) =>
  Struct.makeReducer<A>(canonicalFields(reducers));
