// Effect's `Pool` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes. Each item's
// acquisition runs under a lease holder so Phase M `Within` cannot park
// release on an outer lexical owner.
import { type Duration, Effect, Option, Pool, type Scope } from "effect";

import type { Data } from "../data.ts";
import { tag } from "@flick-lang/runtime/tags";
import { acquireHolder, retire } from "./lease.ts";

export * from "effect/Pool";

const CREATION = /*#__PURE__*/ tag("Pool.TimeToLiveStrategy.Creation");

/** Acquire into a holder, released when Effect's pool item scope closes. */
const held = <A, E, R>(acquire: Effect.Effect<A, E, R>): Effect.Effect<A, E, Exclude<R, Scope.Scope> | Scope.Scope> =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const holder = yield* acquireHolder(acquire, restore);
      yield* Effect.addFinalizer(() => retire(holder));
      return holder.value;
    }),
  );

/** `Pool.make`: the options record with `Option` fields. */
export const flickMake = <A, E, R>(options: {
  readonly acquire: Effect.Effect<A, E, R>;
  readonly size: number;
  readonly concurrency: Option.Option<number>;
  readonly targetUtilization: Option.Option<number>;
}): Effect.Effect<Pool.Pool<A, E>, never, Exclude<R, Scope.Scope> | Scope.Scope> =>
  Pool.make({
    acquire: held(options.acquire),
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
}): Effect.Effect<Pool.Pool<A, E>, never, Exclude<R, Scope.Scope> | Scope.Scope> =>
  Pool.makeWithTTL({
    acquire: held(options.acquire),
    min: options.min,
    max: options.max,
    concurrency: Option.getOrUndefined(options.concurrency),
    targetUtilization: Option.getOrUndefined(options.targetUtilization),
    timeToLive: options.timeToLive,
    timeToLiveStrategy: Option.getOrUndefined(
      Option.map(options.timeToLiveStrategy, (s) => (s._tag === CREATION ? "creation" : "usage")),
    ),
  });
