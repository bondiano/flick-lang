// Effect's `Clock` for the bindings of the standard library: its reads as
// functions that return the effect, so each call reads the clock anew.
import { Clock, type Effect } from "effect";

export * from "effect/Clock";

export const flickMillisOf = (clock: Clock.Clock): Effect.Effect<number> => clock.currentTimeMillis;
export const flickNanosOf = (clock: Clock.Clock): Effect.Effect<bigint> => clock.currentTimeNanos;
export const flickMonotonicNanosOf = (clock: Clock.Clock): Effect.Effect<bigint> => clock.monotonicTimeNanos;

export const flickCurrentTimeMillis = (): Effect.Effect<number> => Clock.currentTimeMillis;
export const flickCurrentTimeNanos = (): Effect.Effect<bigint> => Clock.currentTimeNanos;
export const flickMonotonicTimeNanos = (): Effect.Effect<bigint> => Clock.monotonicTimeNanos;
