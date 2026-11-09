// A combiner folds two values without an identity element.
import { GT, LT } from "../../core/data.ts";
interface Combiner<A> { readonly combine: (a: A, b: A) => A }
export const flickCombine = <A>(combiner: Combiner<A>, a: A, b: A): A => combiner.combine(a, b);
export const make = <A>(combine: (a: A, b: A) => A): Combiner<A> => ({ combine });
export const flip = <A>(combiner: Combiner<A>): Combiner<A> => make((a, b) => combiner.combine(b, a));
export const flickMin = <A>(compare: (a: A, b: A) => { readonly _tag: string }): Combiner<A> =>
  make((a, b) => compare(a, b)._tag === GT._tag ? b : a);
export const flickMax = <A>(compare: (a: A, b: A) => { readonly _tag: string }): Combiner<A> =>
  make((a, b) => compare(a, b)._tag === LT._tag ? b : a);
export const first = <A>(): Combiner<A> => make((a) => a);
export const last = <A>(): Combiner<A> => make((_a, b) => b);
export const constant = <A>(value: A): Combiner<A> => make(() => value);
export const intercalate = <A>(separator: A) => (combiner: Combiner<A>): Combiner<A> =>
  make((a, b) => combiner.combine(combiner.combine(a, separator), b));
