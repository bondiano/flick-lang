// Effect's `ScopedCache` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes. The cache
// keeps each value with a lease count: a value is used only under a lease,
// and dropping its entry releases it once the last lease ends.
import { type Duration, Effect, Exit, Option, Scope, ScopedCache } from "effect";

import { type TimeToLive, nativeTimeToLive } from "./Cache.ts";
import { type Holder, acquireHolder, lease, leased, plain, retire } from "./lease.ts";

export * from "effect/ScopedCache";

/** A lookup whose acquisitions are released when its entry is dropped and no lease holds them. */
const leasedLookup =
  <K, A, E, R>(lookup: (key: K) => Effect.Effect<A, E, R>) =>
  (key: K): Effect.Effect<Holder<A>, E, Exclude<R, Scope.Scope> | Scope.Scope> =>
    // the holder is registered under the mask it was acquired in, as `acquireRelease` does
    Effect.uninterruptibleMask((restore) =>
      Effect.gen(function* () {
        const entry = yield* Scope.Scope;
        const holder = yield* acquireHolder(lookup(key), restore);
        yield* Scope.addFinalizer(entry, retire(holder));
        return holder;
      }),
    );

/** `ScopedCache.make`: the options record with `Option` fields. */
export const flickMake = <K, A, E, R>(options: {
  readonly lookup: (key: K) => Effect.Effect<A, E, R>;
  readonly capacity: number;
  readonly timeToLive: Option.Option<Duration.Duration>;
}): Effect.Effect<ScopedCache.ScopedCache<K, Holder<A>, E>, never, Exclude<R, Scope.Scope> | Scope.Scope> =>
  ScopedCache.make<K, Holder<A>, E, Exclude<R, Scope.Scope>, "construction">({
    lookup: leasedLookup(options.lookup),
    capacity: options.capacity,
    timeToLive: Option.getOrUndefined(options.timeToLive),
  });

/** `ScopedCache.make_with`: a Flick `Exit` of the value for the time to live. */
export const flickMakeWith = <K, A, E, R>(options: {
  readonly lookup: (key: K) => Effect.Effect<A, E, R>;
  readonly capacity: number;
  readonly timeToLive: Option.Option<TimeToLive<K, A>>;
}): Effect.Effect<ScopedCache.ScopedCache<K, Holder<A>, E>, never, Exclude<R, Scope.Scope> | Scope.Scope> =>
  ScopedCache.makeWith<K, Holder<A>, E, Exclude<R, Scope.Scope>, "construction">({
    lookup: leasedLookup(options.lookup),
    capacity: options.capacity,
    timeToLive: Option.getOrUndefined(
      Option.map(options.timeToLive, (f) => (exit: Exit.Exit<Holder<A>, E>, key: K) =>
        nativeTimeToLive<K, A, E>(f)(Exit.map(exit, (holder) => holder.value), key),
      ),
    ),
  });

/** Runs `use` with the value an optional lookup found, under a lease. */
const optional = <A, B, E, R, E2>(
  get: Effect.Effect<Option.Option<Holder<A>>, E2>,
  use: (value: Option.Option<A>) => Effect.Effect<B, E, R>,
): Effect.Effect<B, E | E2, R> =>
  Effect.flatMap(get, (found) =>
    Option.isNone(found)
      ? use(Option.none())
      : lease(found.value, (value) => use(Option.some(value)), () => optional(get, use)),
  );

/** `ScopedCache.use key cache f`: looks `key` up if it is missing or expired. */
export const flickUse = <K, A, B, E, E2, R>(
  key: K,
  self: ScopedCache.ScopedCache<K, Holder<A>, E>,
  f: (value: A) => Effect.Effect<B, E2, R>,
): Effect.Effect<B, E | E2, R> => leased(ScopedCache.get(self, key), f);

/** `ScopedCache.use_option key cache f`: only an unexpired cached value. */
export const flickUseOption = <K, A, B, E, E2, R>(
  key: K,
  self: ScopedCache.ScopedCache<K, Holder<A>, E>,
  f: (value: Option.Option<A>) => Effect.Effect<B, E2, R>,
): Effect.Effect<B, E | E2, R> => optional(ScopedCache.getOption(self, key), f);

/** `ScopedCache.use_success key cache f`: only a completed successful lookup. */
export const flickUseSuccess = <K, A, B, E, E2, R>(
  key: K,
  self: ScopedCache.ScopedCache<K, Holder<A>, E>,
  f: (value: Option.Option<A>) => Effect.Effect<B, E2, R>,
): Effect.Effect<B, E2, R> => optional(ScopedCache.getSuccess(self, key), f);

/** `ScopedCache.refresh key cache`: the previous value is released once no lease holds it. */
export const flickRefresh =
  <K>(key: K) =>
  <A, E>(self: ScopedCache.ScopedCache<K, Holder<A>, E>): Effect.Effect<void, E> =>
    Effect.asVoid(ScopedCache.refresh(self, key));

/** `ScopedCache.set key value cache`: a value that needs no release. */
export const flickSet =
  <K, A>(key: K, value: A) =>
  <E>(self: ScopedCache.ScopedCache<K, Holder<A>, E>): Effect.Effect<void> =>
    ScopedCache.set(self, key, plain(value));

/** `ScopedCache.invalidate_when key f cache`. */
export const flickInvalidateWhen =
  <K, A>(key: K, f: (value: A) => boolean) =>
  <E>(self: ScopedCache.ScopedCache<K, Holder<A>, E>): Effect.Effect<boolean> =>
    ScopedCache.invalidateWhen(self, key, (holder) => f(holder.value));
