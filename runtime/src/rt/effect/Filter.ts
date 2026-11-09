// A filter is a function from an input to a Result. Combinators preserve the
// first failure, except `or`, which tries the fallback on failure.
import { equals as sameValue, type Option, type Result, err, ok, none, some } from "../../core/data.ts";

type Filter<I, A, E> = (input: I) => Result<A, E>;
export const make = <I, A, E>(f: Filter<I, A, E>): Filter<I, A, E> => f;
export const makeEffect = <I, A, E>(f: Filter<I, A, E>): Filter<I, A, E> => f;
export const mapFail = <I, A, E, F>(f: (error: E) => F) => (self: Filter<I, A, E>): Filter<I, A, F> =>
  (input) => {
    const result = self(input);
    return result._tag === "Failure" ? err(f(result.failure)) : result;
  };
export const fromPredicate = <A>(predicate: (value: A) => boolean): Filter<A, A, A> =>
  (value) => predicate(value) ? ok(value) : err(value);
export const fromPredicateOption = <A, B>(f: (value: A) => Option<B>): Filter<A, B, A> =>
  (value) => {
    const option = f(value);
    return option._tag === "Some" ? ok(option.value) : err(value);
  };
export const toPredicate = <I, A, E>(self: Filter<I, A, E>) => (input: I): boolean => self(input)._tag === "Success";
export const toOption = <I, A, E>(self: Filter<I, A, E>) => (input: I): Option<A> => {
  const result = self(input);
  return result._tag === "Success" ? some(result.success) : none;
};
export const toResult = <I, A, E>(self: Filter<I, A, E>): Filter<I, A, E> => self;
export const equalsStrict = <A>(value: A): Filter<A, A, A> => (input) => input === value ? ok(input) : err(input);
export const equals = <A>(value: A): Filter<A, A, A> => (input) => sameValue(input, value) ? ok(input) : err(input);
export const or = <I, A, B, E, F>(that: Filter<I, B, F>) => (self: Filter<I, A, E>): Filter<I, A | B, F> =>
  (input) => {
    const first = self(input);
    return first._tag === "Success" ? first : that(input);
  };
export const zipWith = <I, A, B, C, E>(right: Filter<I, B, E>, combine: (a: A, b: B) => C) =>
  (left: Filter<I, A, E>): Filter<I, C, E> => (input) => {
    const first = left(input);
    if (first._tag === "Failure") return first;
    const second = right(input);
    return second._tag === "Failure" ? second : ok(combine(first.success, second.success));
  };
export const zip = <I, A, B, E>(right: Filter<I, B, E>) =>
  (left: Filter<I, A, E>): Filter<I, readonly [A, B], E> => zipWith(right, (a: A, b: B) => [a, b] as const)(left);
export const andLeft = <I, A, B, E>(right: Filter<I, B, E>) =>
  (left: Filter<I, A, E>): Filter<I, A, E> => zipWith(right, (a: A) => a)(left);
export const andRight = <I, A, B, E>(right: Filter<I, B, E>) =>
  (left: Filter<I, A, E>): Filter<I, B, E> => zipWith(right, (_a: A, b: B) => b)(left);
export const compose = <I, A, B, E>(right: Filter<A, B, E>) =>
  (left: Filter<I, A, E>): Filter<I, B, E> => (input) => {
    const first = left(input);
    return first._tag === "Failure" ? first : right(first.success);
  };
export const composePassthrough = <I, A, B, E, F>(right: Filter<A, B, F>) =>
  (left: Filter<I, A, E>): Filter<I, B, I> => (input) => {
    const first = left(input);
    if (first._tag === "Failure") return err(input);
    const second = right(first.success);
    return second._tag === "Failure" ? err(input) : second;
  };
