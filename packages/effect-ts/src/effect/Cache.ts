// Effect's `Cache` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Cache, type Duration, Effect, type Exit, Option } from "effect";

import type { FlickExit } from "../cause.ts";
import { flickExit } from "../stdlib/outcome.ts";

export * from "effect/Cache";

/**
 * A time to live of each entry, from how its lookup ended and its key: a
 * curried Flick function, as it sits in a record.
 */
export type TimeToLive<K, A> = (exit: FlickExit<A>) => (key: K) => Duration.Duration;

/** Effect's time to live of a Flick one. */
export const nativeTimeToLive =
  <K, A, E>(f: TimeToLive<K, A>) =>
  (exit: Exit.Exit<A, E>, key: K): Duration.Duration =>
    f(flickExit(exit))(key);

/** `Cache.make`: the options record with `Option` fields. */
export const flickMake = <K, A, E, R>(options: {
  readonly lookup: (key: K) => Effect.Effect<A, E, R>;
  readonly capacity: number;
  readonly timeToLive: Option.Option<Duration.Duration>;
}): Effect.Effect<Cache.Cache<K, A, E>, never, R> =>
  Cache.make({
    lookup: options.lookup,
    capacity: options.capacity,
    timeToLive: Option.getOrUndefined(options.timeToLive),
  });

/** `Cache.make_with`: a Flick `Exit` for the time to live. */
export const flickMakeWith = <K, A, E, R>(
  lookup: (key: K) => Effect.Effect<A, E, R>,
  options: { readonly capacity: number; readonly timeToLive: Option.Option<TimeToLive<K, A>> },
): Effect.Effect<Cache.Cache<K, A, E>, never, R> =>
  Cache.makeWith(lookup, {
    capacity: options.capacity,
    timeToLive: Option.getOrUndefined(Option.map(options.timeToLive, nativeTimeToLive<K, A, E>)),
  });

export const flickKeys = <K, A, E>(self: Cache.Cache<K, A, E>): Effect.Effect<Array<K>> =>
  Effect.map(Cache.keys(self), (keys) => Array.from(keys));

export const flickValues = <K, A, E>(self: Cache.Cache<K, A, E>): Effect.Effect<Array<A>> =>
  Effect.map(Cache.values(self), (values) => Array.from(values));

export const flickEntries = <K, A, E>(self: Cache.Cache<K, A, E>): Effect.Effect<Array<[K, A]>> =>
  Effect.map(Cache.entries(self), (entries) => Array.from(entries));
