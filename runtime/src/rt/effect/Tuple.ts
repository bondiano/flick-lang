// Element-wise algebra over Flick tuples (JavaScript arrays).
import { item } from "../../internal.ts";
interface Combiner<A> { readonly combine: (a: A, b: A) => A }
interface Reducer<A> extends Combiner<A> { readonly initial: A }
export const makeCombiner = (parts: ReadonlyArray<Combiner<unknown>>): Combiner<ReadonlyArray<unknown>> => ({
  combine: (a, b) => parts.map((part, index) => part.combine(item(a, index), item(b, index))),
});
export const makeReducer = (parts: ReadonlyArray<Reducer<unknown>>): Reducer<ReadonlyArray<unknown>> => ({
  initial: parts.map((part) => part.initial),
  combine: makeCombiner(parts).combine,
});
