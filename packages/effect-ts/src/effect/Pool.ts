// Effect's `Pool` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes. Each item's
// acquisition runs under a lease holder so Phase M `Within` cannot park
// release on an outer lexical owner.
import { type Duration, Effect, Exit, Option, Pool, Scope } from "effect";

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

/** Whether a lease ended with a defect or an interruption, which may have broken its item. */
const broken = (exit: Exit.Exit<unknown, unknown>): boolean => Exit.hasDies(exit) || Exit.hasInterrupts(exit);

/**
 * `Pool.get`: an item the scope holds; when the scope ends with a defect or an
 * interruption, the pool takes it back invalidated and acquires another.
 */
export const get = <A, E>(self: Pool.Pool<A, E>): Effect.Effect<A, E, Scope.Scope> =>
  Effect.tap(Pool.get(self), (item) =>
    Effect.flatMap(Effect.scope, (scope) =>
      Scope.addFinalizerExit(scope, (exit) => (broken(exit) ? Pool.invalidate(self, item) : Effect.void)),
    ),
  );

/** `Pool.use`: `f` with an item, invalidated as by `get` when `f` ends with a defect or an interruption. */
export const use =
  <A, B, E2, R2>(f: (item: A) => Effect.Effect<B, E2, R2>) =>
  <E>(self: Pool.Pool<A, E>): Effect.Effect<B, E | E2, Exclude<R2, Scope.Scope>> =>
    Effect.scoped(Effect.flatMap(get(self), f));
