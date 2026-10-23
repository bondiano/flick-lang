// Effect's `Function` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: Flick
// functions are curried, Effect's `flip` and `untupled` spread arguments.
export * from "effect/Function";

/** `Function.flip`: `f` of two arguments, curried, with them swapped. */
export const flickFlip =
  <A, B, C>(f: (a: A, b: B) => C) =>
  (b: B) =>
  (a: A): C =>
    f(a, b);

/** `Function.untupled`: `f` of a pair, curried over the two parts. */
export const flickUntupled =
  <A, B, C>(f: (pair: readonly [A, B]) => C) =>
  (a: A) =>
  (b: B): C =>
    f([a, b]);
