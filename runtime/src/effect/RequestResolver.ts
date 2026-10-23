// Effect's `RequestResolver` for the bindings of the standard library:
// callbacks inside records arrive curried, and the batch key of `make` is
// Effect's own.
import { Effect, Option, type Request, RequestResolver } from "effect";

import { type TimeToLive, nativeTimeToLive } from "./Cache.ts";
import { type FlickTraceOptions, traceOptions } from "./Layer.ts";
import type { FlickRequest } from "./Request.ts";

export * from "effect/RequestResolver";

type Entry<P, E, A> = Request.Entry<FlickRequest<P, E, A>>;
type Entries<P, E, A> = readonly [Entry<P, E, A>, ...Array<Entry<P, E, A>>];
type Resolver<P, E, A> = RequestResolver.RequestResolver<FlickRequest<P, E, A>>;

/** `RequestResolver.make`: without the batch key. */
export const flickMake = <P, E, A>(runAll: (entries: Entries<P, E, A>) => Effect.Effect<void, E>): Resolver<P, E, A> =>
  RequestResolver.make((entries) => runAll(entries));

/**
 * A batch's delay that yields first: Effect forks the delay before it adds
 * the first entry, so a delay that completes at once would run an empty
 * batch and lose the entry.
 */
const yielding = (delay: Effect.Effect<void>): Effect.Effect<void> => Effect.andThen(Effect.yieldNow, delay);

/** `RequestResolver.set_delay_effect`: the delay yields first. */
export const flickSetDelayEffect =
  (delay: Effect.Effect<void>) =>
  <P, E, A>(self: Resolver<P, E, A>): Resolver<P, E, A> =>
    RequestResolver.setDelayEffect(self, yielding(delay));

/** `RequestResolver.make_with`: the delay yields first. */
export const flickMakeWith = <P, E, A, K>(options: {
  readonly batchKey: (entry: Entry<P, E, A>) => K;
  readonly preCheck: Option.Option<(entry: Entry<P, E, A>) => boolean>;
  readonly delay: Effect.Effect<void>;
  readonly collectWhile: (entries: ReadonlyArray<Entry<P, E, A>>) => boolean;
  readonly runAll: (entries: Entries<P, E, A>) => (key: K) => Effect.Effect<void, E>;
}): Resolver<P, E, A> =>
  RequestResolver.makeWith({
    batchKey: options.batchKey,
    preCheck: Option.getOrUndefined(options.preCheck),
    delay: yielding(options.delay),
    collectWhile: (entries) => options.collectWhile([...entries]),
    // the key is the one `batchKey` gave the batch
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- Effect types the key `unknown`
    runAll: (entries, key) => options.runAll(entries)(key as K),
  });

/** `RequestResolver.make_grouped`. */
export const flickMakeGrouped = <P, E, A, K>(options: {
  readonly key: (entry: Entry<P, E, A>) => K;
  readonly resolver: (entries: Entries<P, E, A>) => (key: K) => Effect.Effect<void, E>;
}): Resolver<P, E, A> =>
  RequestResolver.makeGrouped({
    key: options.key,
    resolver: (entries, key) => options.resolver(entries)(key),
  });

/** `RequestResolver.with_span`. */
export const flickWithSpan =
  (name: string, options: FlickTraceOptions) =>
  <P, E, A>(self: Resolver<P, E, A>): Resolver<P, E, A> =>
    RequestResolver.withSpan(self, name, traceOptions(options));

/** `RequestResolver.with_cache`. */
export const flickWithCache =
  (options: { readonly capacity: number; readonly strategy: Option.Option<"lru" | "fifo"> }) =>
  <P, E, A>(self: Resolver<P, E, A>): Effect.Effect<Resolver<P, E, A>> =>
    RequestResolver.withCache(self, { capacity: options.capacity, strategy: Option.getOrUndefined(options.strategy) });

/** `RequestResolver.as_cache`: a Flick `Exit` for the time to live. */
export const flickAsCache =
  <P, E, A>(options: {
    readonly capacity: number;
    readonly timeToLive: Option.Option<TimeToLive<FlickRequest<P, E, A>, A>>;
  }) =>
  (self: Resolver<P, E, A>) =>
    RequestResolver.asCache(self, {
      capacity: options.capacity,
      timeToLive: Option.getOrUndefined(Option.map(options.timeToLive, (f) => nativeTimeToLive<FlickRequest<P, E, A>, A, E>(f))),
    });
