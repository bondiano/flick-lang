// Effect's `FiberSet` for the bindings of the standard library, and the
// adapters that keep its children in the owner of its region.
import { Effect, FiberSet } from "effect";

import type { FlickFiber } from "../owner.ts";
import { flickFiber, inOwner, owned } from "../stdlib/fibers.ts";

export * from "effect/FiberSet";

interface RunOptions {
  readonly propagateInterruption: boolean;
  readonly startImmediately: boolean;
}

export const flickMake = <A, E>() => owned(FiberSet.make<A, E>(), FiberSet.clear);

export const flickAdd =
  (propagateInterruption: boolean) =>
  <A, E>(fiber: FlickFiber<A, E>) =>
  (self: FiberSet.FiberSet<A, E>): Effect.Effect<void> =>
    FiberSet.add(self, fiber.fiber, { propagateInterruption });

export const flickRun =
  (options: RunOptions) =>
  <A, E, R>(task: Effect.Effect<A, E, R>) =>
  (self: FiberSet.FiberSet<A, E>) =>
    Effect.map(FiberSet.run(self, inOwner(self, task), options), flickFiber);
