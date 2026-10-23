// Effect's `Pool` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Duration, type Effect, Option, Pool, type Scope } from "effect";

import type { Data } from "../data.ts";
import { tag } from "../tags.ts";

export * from "effect/Pool";

const CREATION = tag("Pool.TimeToLiveStrategy.Creation");

/** `Pool.make`: the options record with `Option` fields. */
export const flickMake = <A, E, R>(options: {
  readonly acquire: Effect.Effect<A, E, R>;
  readonly size: number;
  readonly concurrency: Option.Option<number>;
  readonly targetUtilization: Option.Option<number>;
}): Effect.Effect<Pool.Pool<A, E>, never, R | Scope.Scope> =>
  Pool.make({
    acquire: options.acquire,
    size: options.size,
    concurrency: Option.getOrUndefined(options.concurrency),
    targetUtilization: Option.getOrUndefined(options.targetUtilization),
  });

/** `Pool.make_with_ttl`: `Option` fields and a Flick `TimeToLiveStrategy`. */
export const flickMakeWithTtl = <A, E, R>(options: {
  readonly acquire: Effect.Effect<A, E, R>;
  readonly min: number;
  readonly max: number;
  readonly concurrency: Option.Option<number>;
  readonly targetUtilization: Option.Option<number>;
  readonly timeToLive: Duration.Duration;
  readonly timeToLiveStrategy: Option.Option<Data>;
}): Effect.Effect<Pool.Pool<A, E>, never, R | Scope.Scope> =>
  Pool.makeWithTTL({
    acquire: options.acquire,
    min: options.min,
    max: options.max,
    concurrency: Option.getOrUndefined(options.concurrency),
    targetUtilization: Option.getOrUndefined(options.targetUtilization),
    timeToLive: options.timeToLive,
    timeToLiveStrategy: Option.getOrUndefined(
      Option.map(options.timeToLiveStrategy, (s) => (s._tag === CREATION ? "creation" : "usage")),
    ),
  });
