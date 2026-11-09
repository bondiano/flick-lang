// Effect's `Struct` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Combiner, type Reducer, Struct } from "effect";

export * from "effect/Struct";

/** `Combiner.struct`: the `Record.Map` dictionary is erased; Flick records are exact, so no `omitKeyWhen`. */
export const flickMakeCombiner = <A>(_map: unknown, combiners: { readonly [K in keyof A]: Combiner.Combiner<A[K]> }) =>
  Struct.makeCombiner<A>(combiners);

/** `Reducer.struct`: as `flickMakeCombiner`. */
export const flickMakeReducer = <A>(_map: unknown, reducers: { readonly [K in keyof A]: Reducer.Reducer<A[K]> }) =>
  Struct.makeReducer<A>(reducers);
