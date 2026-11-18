// Effect's `Random` for the bindings of the standard library: its draws as
// functions that return the effect, so each call draws anew.
import { Random, type Effect } from "effect";

export * from "effect/Random";

export const flickNext = (): Effect.Effect<number> => Random.next;
