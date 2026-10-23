// Effect's `Combiner` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Combiner } from "effect";

import type { Ordering } from "../data.ts";
import { toOrder } from "./ordering.ts";

export * from "effect/Combiner";

/** `Combiner.combine c self that`: runs the combiner. */
export const flickCombine = <A>(combiner: Combiner.Combiner<A>, self: A, that: A): A => combiner.combine(self, that);

/** `Combiner.min`: of a Flick order. */
export const flickMin = <A>(compare: (self: A, that: A) => Ordering): Combiner.Combiner<A> => Combiner.min(toOrder(compare));

/** `Combiner.max`: of a Flick order. */
export const flickMax = <A>(compare: (self: A, that: A) => Ordering): Combiner.Combiner<A> => Combiner.max(toOrder(compare));
