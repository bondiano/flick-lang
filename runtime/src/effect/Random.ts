// Effect's `Random` for the bindings of the standard library: its draws as
// functions that return the effect, so each call draws anew.
import { Option, Random, type Effect } from "effect";

export * from "effect/Random";

export const flickNext = (): Effect.Effect<number> => Random.next;
export const flickNextBoolean = (): Effect.Effect<boolean> => Random.nextBoolean;
// Flick's `Int` is 32-bit, so the draw is over that range rather than the safe integers.
export const flickNextInt = (): Effect.Effect<number> => Random.nextIntBetween(-0x80000000, 0x7fffffff);

/** `Random.next_int_between`: the options record with an `Option` field. */
export const flickNextIntBetween = (
  options: { readonly halfOpen: Option.Option<boolean> },
  min: number,
  max: number,
): Effect.Effect<number> => Random.nextIntBetween(min, max, { halfOpen: Option.getOrElse(options.halfOpen, () => false) });
