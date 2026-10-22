// Races: the winner's result, losers interrupted and awaited; their exits
// are not added to the result.
import { Cause, Duration, Effect } from "effect";

/** The `_tag` of the stdlib `TimeoutError`. */
export const TIMEOUT_ERROR = "Prelude.TimeoutError.TimeoutError";

const timeoutError = Object.freeze({ _tag: TIMEOUT_ERROR });

/** `race_success`: the first branch to succeed. */
export const raceSuccess = (a, b) => Effect.race(a, b);

/** `race_first`: the first branch to end, however it ends. */
export const raceFirst = (a, b) => Effect.raceFirst(a, b);

/** `timeout millis effect`: fails with `TimeoutError` when time runs out first. */
export const timeout = (millis, effect) =>
  Effect.mapError(Effect.timeout(effect, Duration.millis(millis)), (error) =>
    Cause.isTimeoutError(error) ? timeoutError : error,
  );

/** `timeout_option millis effect`: `None` when time runs out first. */
export const timeoutOption = (millis, effect) =>
  Effect.timeoutOption(effect, Duration.millis(millis));
