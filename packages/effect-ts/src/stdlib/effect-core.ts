// Adapters of Effect's `Effect` for the parts `Effect.Core` and
// `Effect.Concurrency`: options records from positional Flick arguments,
// Flick's `Concurrency`, and Effect's errors as Flick errors.
import { Cause, type Duration, Effect, Exit, Function, Result, type Schedule, type Scope } from "effect";

import { type Failure, dropSignalsOnDie, exitOf, hasRealDie, hasSignal, passOn, stripSignals, unwinds } from "../cause.ts";
import type { Data } from "../data.ts";
import { tag, constructorName } from "@flick-lang/runtime/tags";
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

const partitionExits = <A, B, E, R>(
  c: Data,
  f: (a: A) => Effect.Effect<B, E, R>,
  elements: ReadonlyArray<A>,
): Effect.Effect<[Array<B>, Array<E>], never, R> =>
  Effect.map(
    Effect.forEach(
      elements,
      (a) =>
        Effect.flatMap(Effect.exit(f(a)), (exit): Effect.Effect<Result.Result<B, E>> => {
          if (Exit.isSuccess(exit)) return Effect.succeed(Result.succeed(exit.value));
          const reason =
            hasRealDie(exit.cause) || unwinds(exit.cause)
              ? undefined
              : exit.cause.reasons.find(Cause.isFailReason);
          return reason === undefined
            ? passOn(hasRealDie(exit.cause) && hasSignal(exit.cause) ? stripSignals(exit.cause) : exit.cause)
            : Effect.succeed(Result.fail(reason.error));
        }),
      { concurrency: concurrency(c) },
    ),
    (results) => {
      const passes: Array<B> = [];
      const fails: Array<E> = [];
      for (const r of results) {
        if (Result.isSuccess(r)) passes.push(r.success);
        else fails.push(r.failure);
      }
      return [passes, fails];
    },
  );

export const flickPartition =
  <A, B, E, R>(c: Data, f: (a: A) => Effect.Effect<B, E, R>) =>
  (elements: ReadonlyArray<A>): Effect.Effect<[Array<B>, Array<E>], never, R> =>
    partitionExits(c, f, elements);

export const flickValidate =
  <A, B, E, R>(c: Data, f: (a: A) => Effect.Effect<B, E, R>) =>
  (elements: ReadonlyArray<A>): Effect.Effect<Array<B>, { readonly _tag: string; readonly _0: Array<E> }, R> =>
    Effect.flatMap(partitionExits(c, f, elements), ([passes, fails]) =>
      fails.length > 0 ? Effect.fail({ _tag: VALIDATION, _0: fails }) : Effect.succeed(passes),
    );

/** `Effect.first_success_of`: propagates handler unwinding instead of treating it as a typed error. */
export const flickFirstSuccessOf = <A, E, R>(
  effects: Iterable<Effect.Effect<A, E, R>>,
): Effect.Effect<A, E, R> =>
  Effect.suspend(() => {
    const items = Array.from(effects);
    if (items.length === 0) {
      return Effect.die(new Error("Received an empty collection of effects"));
    }
    const step = (index: number): Effect.Effect<A, E, R> => {
      const current = items[index];
      if (current === undefined) {
        return Effect.die(new Error("Received an empty collection of effects"));
      }
      if (index === items.length - 1) return current;
      return Effect.catchCause(current, (cause) =>
        !Cause.hasFails(cause) || hasRealDie(cause) || unwinds(cause)
          ? passOn(hasRealDie(cause) && hasSignal(cause) ? stripSignals(cause) : cause)
          : step(index + 1),
      );
    };
    return step(0);
  });

/** `Effect.eventually`: retries typed failures only; handler unwinding propagates immediately. */
export const flickEventually = <A, E, R>(
  self: Effect.Effect<A, E, R>,
): Effect.Effect<A, E, R> => {
  const loop: Effect.Effect<A, E, R> = Effect.catchCause(self, (cause) =>
    !Cause.hasFails(cause) || hasRealDie(cause) || unwinds(cause)
      ? passOn(hasRealDie(cause) && hasSignal(cause) ? stripSignals(cause) : cause)
      : Effect.flatMap(Effect.yieldNow, () => loop),
  );
  return loop;
};

/** `Effect.ensuring`: strips handler signals when a defect is also present so defects are neither swallowed nor polluted. */
export const flickEnsuring: {
  <R2>(finalizer: Effect.Effect<unknown, never, R2>): <A, E, R>(self: Effect.Effect<A, E, R>) => Effect.Effect<A, E, R | R2>;
  <A, E, R, R2>(self: Effect.Effect<A, E, R>, finalizer: Effect.Effect<unknown, never, R2>): Effect.Effect<A, E, R | R2>;
} = Function.dual(
  2,
  <A, E, R, R2>(self: Effect.Effect<A, E, R>, finalizer: Effect.Effect<unknown, never, R2>): Effect.Effect<A, E, R | R2> =>
    dropSignalsOnDie(Effect.ensuring(self, finalizer)),
);

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

