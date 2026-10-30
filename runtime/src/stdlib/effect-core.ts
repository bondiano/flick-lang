// Adapters of Effect's `Effect` for the parts `Effect.Core` and
// `Effect.Concurrency`: options records from positional Flick arguments,
// Flick's `Concurrency`, and Effect's errors as Flick errors.
import { type Duration, Effect, type Result, type Schedule, type Scope } from "effect";

import { type Failure, exitOf } from "../cause.ts";
import type { Data } from "../data.ts";
import { tag, constructorName } from "../tags.ts";
import { causeOf, noSuchElement } from "../effect/Cause.ts";
import { type FlickResource, flickManual, flickRefresh } from "../effect/Resource.ts";

/** Effect's `Concurrency` of a Flick `Effect.Concurrency`. */
const concurrency = (c: Data): number | "unbounded" | undefined => {
  switch (constructorName(c._tag)) {
    case "Bounded": {
      const n = c["_0"];
      return typeof n === "number" ? n : undefined;
    }
    case "Unbounded":
      return "unbounded";
    default:
      return undefined;
  }
};

const VALIDATION = /*#__PURE__*/ tag("Effect.Collections.ValidationError.ValidationError");

export const flickNever = (): Effect.Effect<never> => Effect.never;
export const flickYieldNow = (): Effect.Effect<void> => Effect.yieldNow;
export const flickFiberId = (): Effect.Effect<number> => Effect.fiberId;
export const flickExit = exitOf;
export const flickFailCauseSync = (f: () => Failure): Effect.Effect<never, unknown> =>
  Effect.failCauseSync(() => causeOf(f()));

export const flickTimeoutOrElse =
  <A2, E2, R2>(duration: Duration.Duration, orElse: Effect.Effect<A2, E2, R2>) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.timeoutOrElse(self, { duration, orElse: () => orElse });

export const flickReplicateEffect =
  (c: Data, n: number) =>
  <A, E, R>(self: Effect.Effect<A, E, R>) =>
    Effect.replicateEffect(self, n, { concurrency: concurrency(c) });

export const flickForEach =
  <A, B, E, R>(c: Data, f: (a: A) => Effect.Effect<B, E, R>) =>
  (elements: ReadonlyArray<A>) =>
    Effect.forEach(elements, (a) => f(a), { concurrency: concurrency(c) });

export const flickAll =
  (c: Data) =>
  <A, E, R>(effects: ReadonlyArray<Effect.Effect<A, E, R>>) =>
    Effect.all(effects, { concurrency: concurrency(c) });

export const flickPartition =
  <A, B, E, R>(c: Data, f: (a: A) => Effect.Effect<B, E, R>) =>
  (elements: ReadonlyArray<A>) =>
    Effect.partition(elements, (a) => f(a), { concurrency: concurrency(c) });

export const flickValidate =
  <A, B, E, R>(c: Data, f: (a: A) => Effect.Effect<B, E, R>) =>
  (elements: ReadonlyArray<A>) =>
    Effect.mapError(Effect.validate(elements, (a) => f(a), { concurrency: concurrency(c) }), (errors) => ({
      _tag: VALIDATION,
      _0: errors,
    }));

export const flickFilter =
  <A, E, R>(c: Data, f: (a: A) => Effect.Effect<boolean, E, R>) =>
  (elements: ReadonlyArray<A>) =>
    Effect.filter(elements, (a) => f(a), { concurrency: concurrency(c) });

export const flickFilterMapEffect =
  <A, B, X, E, R>(c: Data, f: (a: A) => Effect.Effect<Result.Result<B, X>, E, R>) =>
  (elements: ReadonlyArray<A>) =>
    Effect.filterMapEffect(elements, (a) => f(a), { concurrency: concurrency(c) });

export const flickHead = <A, E, R>(self: Effect.Effect<ReadonlyArray<A>, E, R>) =>
  Effect.mapError(Effect.head(self), noSuchElement);

export const flickReduce =
  <Z, A, E, R>(zero: Z, f: (z: Z, a: A) => Effect.Effect<Z, E, R>) =>
  (elements: ReadonlyArray<A>) =>
    Effect.reduce(elements, () => zero, (z, a) => f(z, a));

/** `Effect.resource_auto`: Effect's `Resource.auto` over the leased `Resource`, the schedule first. */
export const flickResourceAuto =
  <O, E2, R2>(schedule: Schedule.Schedule<O, unknown, E2, R2>) =>
  <A, E, R>(
    acquire: Effect.Effect<A, E, R>,
  ): Effect.Effect<FlickResource<A, E>, never, Exclude<R, Scope.Scope> | R2 | Scope.Scope> =>
    Effect.tap(flickManual(acquire), (self) => Effect.forkScoped(Effect.repeat(flickRefresh(self), schedule)));

