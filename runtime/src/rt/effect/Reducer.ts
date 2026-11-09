// A reducer has an initial value and a binary combine operation.
interface Reducer<A> { readonly initial: A; readonly combine: (a: A, b: A) => A }
export const make = <A>(combine: (a: A, b: A) => A, initial: A): Reducer<A> => ({ combine, initial });
export const flickCombine = <A>(reducer: Reducer<A>, a: A, b: A): A => reducer.combine(a, b);
export const flickInitialValue = <A>(reducer: Reducer<A>): A => reducer.initial;
export const flickCombineAll = <A>(reducer: Reducer<A>, values: ReadonlyArray<A>): A =>
  values.reduce(reducer.combine, reducer.initial);
export const flickToCombiner = <A>(reducer: Reducer<A>): { readonly combine: (a: A, b: A) => A } => reducer;
export const flip = <A>(reducer: Reducer<A>): Reducer<A> =>
  make((a, b) => reducer.combine(b, a), reducer.initial);
