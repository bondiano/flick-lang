// Effect's `ScopedCache` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Duration, type Effect, Option, type Scope, ScopedCache } from "effect";

import { type TimeToLive, nativeTimeToLive } from "./Cache.ts";

export * from "effect/ScopedCache";

/** `ScopedCache.make`: the options record with `Option` fields. */
export const flickMake = <K, A, E, R>(options: {
  readonly lookup: (key: K) => Effect.Effect<A, E, R>;
  readonly capacity: number;
  readonly timeToLive: Option.Option<Duration.Duration>;
}): Effect.Effect<ScopedCache.ScopedCache<K, A, E>, never, R | Scope.Scope> =>
  ScopedCache.make<K, A, E, R, "construction">({
    lookup: options.lookup,
    capacity: options.capacity,
    timeToLive: Option.getOrUndefined(options.timeToLive),
  });

/** `ScopedCache.make_with`: a Flick `Exit` for the time to live. */
export const flickMakeWith = <K, A, E, R>(options: {
  readonly lookup: (key: K) => Effect.Effect<A, E, R>;
  readonly capacity: number;
  readonly timeToLive: Option.Option<TimeToLive<K, A>>;
}): Effect.Effect<ScopedCache.ScopedCache<K, A, E>, never, R | Scope.Scope> =>
  ScopedCache.makeWith<K, A, E, R, "construction">({
    lookup: options.lookup,
    capacity: options.capacity,
    timeToLive: Option.getOrUndefined(Option.map(options.timeToLive, nativeTimeToLive<K, A, E>)),
  });
