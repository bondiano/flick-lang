// Effect's `Filter` for the bindings of the standard library.
import { Result } from "effect";
import { sameValue } from "../data.ts";

export * from "effect/Filter";

export const equalsStrict =
  <A>(value: A) =>
  (input: A): Result.Result<A, A> =>
    sameValue(input, value) ? Result.succeed(input) : Result.fail(input);
