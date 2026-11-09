// The native Effect outcomes behind Flick's opaque `Exit e a` and
// `Failure e`, for adapters that hand them back to Effect, and the end of a
// queue as `None`.
import { type Cause, Effect, Exit, Option, Pull } from "effect";

import { type Failure, FlickExit, nativeCause } from "../cause.ts";
import { none, some } from "../data.ts";

/** The native cause of a Flick `Failure e`, typed by the row it fails with. */
export const failureCause = <E>(failure: Failure): Cause.Cause<E> => {
  const cause = nativeCause(failure);
  if (cause === undefined) throw new TypeError("not a Failure of this runtime");
  // a `Failure e` fails only with errors of `e`
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- one Flick type, one error type
  return cause as Cause.Cause<E>;
};

/** The native exit of a Flick `Exit e a`. */
export const nativeExit = <A, E>(exit: FlickExit<A>): Exit.Exit<A, E> => {
  const value = FlickExit.value(exit);
  if (Option.isSome(value)) return Exit.succeed(value.value);
  const failure = FlickExit.failure(exit);
  if (Option.isNone(failure)) throw new TypeError("an Exit is a success or a failure");
  return Exit.failCause(failureCause<E>(failure.value));
};

/** A Flick `Exit e a` of a native exit. */
export const flickExit = <A, E>(exit: Exit.Exit<A, E>): FlickExit<A> => new FlickExit(exit);

/** `Some` of what `effect` gives, `None` once its source is done. */
export const orDone = <A, E, R>(
  effect: Effect.Effect<A, E, R>,
): Effect.Effect<Option.Option<A>, Pull.ExcludeDone<E>, R> =>
  Pull.catchDone(Effect.map(effect, some), () => Effect.succeed(none));
