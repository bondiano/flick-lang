// Effect's `Clock` for the bindings of the standard library: its reads as
// functions that return the effect, so each call reads the clock anew.
import { Clock, type Effect } from "effect";

export * from "effect/Clock";

export const flickCurrentTimeMillis = (): Effect.Effect<number> => Clock.currentTimeMillis;
export const flickCurrentTimeNanos = (): Effect.Effect<bigint> => Clock.currentTimeNanos;
export const flickMonotonicTimeNanos = (): Effect.Effect<bigint> => Clock.monotonicTimeNanos;
