// Helpers of the runtime tests: unwrapping that fails the test instead of
// returning `undefined`.
import assert from "node:assert/strict";

import { type Cause, Exit, Option } from "effect";

import { item } from "@flick-lang/runtime/internal";

/** The value of a `Some`. */
export const value = <A>(option: Option.Option<A>): A => {
  assert.ok(Option.isSome(option), "expected Some");
  return option.value;
};

/** The cause of a failed exit. */
export const causeOf = <A, E>(exit: Exit.Exit<A, E>): Cause.Cause<E> => {
  assert.ok(Exit.isFailure(exit), "expected a failure");
  return exit.cause;
};

/** The element `i` of `xs`, which must exist. */
export const at = <A>(xs: ReadonlyArray<A>, i: number): A => {
  assert.ok(i < xs.length, `no element ${i}`);
  return item(xs, i);
};

/** The typed errors of a failed exit, in order. */
export const errorsOf = <A, E>(exit: Exit.Exit<A, E>): Array<E> =>
  causeOf(exit).reasons.flatMap((r) => (r._tag === "Fail" ? [r.error] : []));
