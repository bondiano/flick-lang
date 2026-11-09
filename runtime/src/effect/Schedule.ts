// Effect's `Schedule` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Effect, Option, Schedule } from "effect";
import type { Duration } from "effect";

export * from "effect/Schedule";

/** `Schedule.up_to`: the options record with `Option` fields. */
export const flickUpTo =
  (options: { readonly duration: Option.Option<Duration.Duration>; readonly times: Option.Option<number> }) =>
  <O, I, E, R>(self: Schedule.Schedule<O, I, E, R>): Schedule.Schedule<O, I, E, R> =>
    Schedule.upTo(self, {
      duration: Option.getOrUndefined(options.duration),
      times: Option.getOrUndefined(options.times),
    });

/** `Schedule.current_metadata`: the metadata of the schedule that repeats the running effect. */
export const flickCurrentMetadata = (): Effect.Effect<Schedule.Metadata> => Effect.service(Schedule.CurrentMetadata);
