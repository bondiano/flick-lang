// Effect's `Filter` for the bindings of the standard library.
import { Result } from "effect";
import { equals as flickEquals, sameValue } from "../data.ts";

export * from "effect/Filter";

export const equals =
  <A>(value: A) =>
  (input: A): Result.Result<A, A> =>
    flickEquals(input, value) ? Result.succeed(input) : Result.fail(input);

export const equalsStrict =
  <A>(value: A) =>
  (input: A): Result.Result<A, A> =>
    sameValue(input, value) ? Result.succeed(input) : Result.fail(input);
