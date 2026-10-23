// Effect's `PubSub` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Effect, Option, PubSub } from "effect";

export * from "effect/PubSub";

/** `PubSub.Bounds`: the capacity and an optional replay. */
interface Bounds {
  readonly capacity: number;
  readonly replay: Option.Option<number>;
}

const bounds = (options: Bounds): { readonly capacity: number; readonly replay: number | undefined } => ({
  capacity: options.capacity,
  replay: Option.getOrUndefined(options.replay),
});

export const flickBounded = <A>(options: Bounds): Effect.Effect<PubSub.PubSub<A>> => PubSub.bounded(bounds(options));
export const flickDropping = <A>(options: Bounds): Effect.Effect<PubSub.PubSub<A>> => PubSub.dropping(bounds(options));
export const flickSliding = <A>(options: Bounds): Effect.Effect<PubSub.PubSub<A>> => PubSub.sliding(bounds(options));

export const flickUnbounded = <A>(options: {
  readonly replay: Option.Option<number>;
}): Effect.Effect<PubSub.PubSub<A>> => PubSub.unbounded({ replay: Option.getOrUndefined(options.replay) });
