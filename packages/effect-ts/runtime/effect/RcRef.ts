// Effect's `RcRef` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Duration, type Effect, Option, RcRef, type Scope } from "effect";

export * from "effect/RcRef";

/** `RcRef.make`: the options record with `Option` fields. */
export const flickMake = <A, E, R>(options: {
  readonly acquire: Effect.Effect<A, E, R>;
  readonly idleTimeToLive: Option.Option<Duration.Duration>;
}): Effect.Effect<RcRef.RcRef<A, E>, never, R | Scope.Scope> =>
  RcRef.make({ acquire: options.acquire, idleTimeToLive: Option.getOrUndefined(options.idleTimeToLive) });
