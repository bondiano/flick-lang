// Races: the winner's result, losers interrupted and awaited; their exits
// are not added to the result.
import { Cause, Duration, Effect, type Option } from "effect";
import { tag } from "./tags.ts";

/** The `_tag` of the stdlib `TimeoutError`. */
export const TIMEOUT_ERROR = tag("Prelude.TimeoutError.TimeoutError");

/** `Prelude.TimeoutError`. */
export type TimeoutError = Readonly<{ _tag: typeof TIMEOUT_ERROR }>;

const timeoutError: TimeoutError = Object.freeze({ _tag: TIMEOUT_ERROR });

/** `race_success`: the first branch to succeed. */
export const raceSuccess = <A, E, R, A2, E2, R2>(
  a: Effect.Effect<A, E, R>,
  b: Effect.Effect<A2, E2, R2>,
): Effect.Effect<A | A2, E | E2, R | R2> => Effect.race(a, b);

/** `race_first`: the first branch to end, however it ends. */
export const raceFirst = <A, E, R, A2, E2, R2>(
  a: Effect.Effect<A, E, R>,
  b: Effect.Effect<A2, E2, R2>,
): Effect.Effect<A | A2, E | E2, R | R2> => Effect.raceFirst(a, b);

/** `timeout millis effect`: fails with `TimeoutError` when time runs out first. */
export const timeout = <A, E, R>(millis: number, effect: Effect.Effect<A, E, R>): Effect.Effect<A, E | TimeoutError, R> =>
  Effect.mapError(Effect.timeout(effect, Duration.millis(millis)), (error) =>
    Cause.isTimeoutError(error) ? timeoutError : error,
  );

/** `timeout_option millis effect`: `None` when time runs out first. */
export const timeoutOption = <A, E, R>(millis: number, effect: Effect.Effect<A, E, R>): Effect.Effect<Option.Option<A>, E, R> =>
  Effect.timeoutOption(effect, Duration.millis(millis));
