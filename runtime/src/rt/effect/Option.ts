// Option algebra on Flick's plain Option ABI.
export * from "../../core/Option.ts";
import { none, some, type Option } from "../../core/data.ts";
interface Combiner<A> { readonly combine: (a: A, b: A) => A }
interface Reducer<A> extends Combiner<A> { readonly initial: A }
export const makeReducer = <A>(combiner: Combiner<A>): Reducer<Option<A>> => ({
  initial: none,
  combine: (a, b) => a._tag === "None" ? b : b._tag === "None" ? a : some(combiner.combine(a.value, b.value)),
});
export const makeCombinerFailFast = <A>(combiner: Combiner<A>): Combiner<Option<A>> => ({
  combine: (a, b) => a._tag === "None" || b._tag === "None" ? none : some(combiner.combine(a.value, b.value)),
});
export const makeReducerFailFast = <A>(reducer: Reducer<A>): Reducer<Option<A>> => ({
  initial: some(reducer.initial),
  combine: makeCombinerFailFast(reducer).combine,
});
