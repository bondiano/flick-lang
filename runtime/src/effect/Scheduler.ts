// Effect's `Scheduler` for the bindings of the standard library, and the
// adapters that read and set its references around a task.
import { Effect, Scheduler } from "effect";

export * from "effect/Scheduler";

/** `Scheduler.make_mixed`: a fresh asynchronous `MixedScheduler`. */
export const flickMakeMixed = (): Scheduler.Scheduler => new Scheduler.MixedScheduler("async");

/** `Scheduler.current`. */
export const flickCurrent = (): Effect.Effect<Scheduler.Scheduler> => Effect.service(Scheduler.Scheduler);

export const flickWithScheduler =
  (scheduler: Scheduler.Scheduler) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, Scheduler.Scheduler, scheduler);

export const flickWithMaxOpsBeforeYield =
  (ops: number) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, Scheduler.MaxOpsBeforeYield, ops);

export const flickWithPreventSchedulerYield =
  (prevent: boolean) =>
  <A, E, R>(self: Effect.Effect<A, E, R>): Effect.Effect<A, E, R> =>
    Effect.provideService(self, Scheduler.PreventSchedulerYield, prevent);

/** `Scheduler.max_ops_before_yield`: the current number. */
export const flickMaxOpsBeforeYield = (): Effect.Effect<number> => Effect.service(Scheduler.MaxOpsBeforeYield);
