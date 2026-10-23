// Effect's `Reducer` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import type { Combiner, Reducer } from "effect";

export * from "effect/Reducer";

/** `Reducer.combine r self that`: runs the reducer's combination. */
export const flickCombine = <A>(reducer: Reducer.Reducer<A>, self: A, that: A): A => reducer.combine(self, that);

/** `Reducer.initial_value`. */
export const flickInitialValue = <A>(reducer: Reducer.Reducer<A>): A => reducer.initialValue;

/** `Reducer.combine_all`: from the initial value, over the list. */
export const flickCombineAll = <A>(reducer: Reducer.Reducer<A>, xs: ReadonlyArray<A>): A => reducer.combineAll(xs);

/** `Reducer.to_combiner`: a reducer is a combiner. */
export const flickToCombiner = <A>(reducer: Reducer.Reducer<A>): Combiner.Combiner<A> => reducer;
