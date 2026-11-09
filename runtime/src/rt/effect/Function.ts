// Functions with the curried ABI of Flick's foreign bindings.
export const constant = <A>(value: A): ((unit: undefined) => A) => () => value;
export const constTrue = (): boolean => true;
export const constFalse = (): boolean => false;
export const constVoid = (): undefined => undefined;
export const flickFlip = <A, B, C>(f: (a: A, b: B) => C) => (b: B) => (a: A): C => f(a, b);
export const compose = <A, B, C>(g: (b: B) => C) => (f: (a: A) => B) => (value: A): C => g(f(value));
export const absurd = (_value: never): never => { throw new Error("absurd: a Never value was supplied"); };
export const tupled = <A, B, C>(f: (a: A, b: B) => C) => (pair: readonly [A, B]): C => f(pair[0], pair[1]);
export const flickUntupled = <A, B, C>(f: (pair: readonly [A, B]) => C) =>
  (a: A) => (b: B): C => f([a, b]);
export const SK = <B>(_first: unknown, second: B): B => second;
