// Effect's `FiberHandle` for the bindings of the standard library, and the
// adapters that keep its child in the owner of its region.
import { Effect, FiberHandle } from "effect";

import type { FlickFiber } from "../owner.ts";
import { flickFiber, flickFiberOption, inOwner, owned } from "../stdlib/fibers.ts";

export * from "effect/FiberHandle";

interface SetOptions {
  readonly onlyIfMissing: boolean;
  readonly propagateInterruption: boolean;
}

interface RunOptions extends SetOptions {
  readonly startImmediately: boolean;
}

export const flickMake = <A, E>() => owned(FiberHandle.make<A, E>(), FiberHandle.clear);

export const flickSet =
  (options: SetOptions) =>
  <A, E>(fiber: FlickFiber<A, E>) =>
  (self: FiberHandle.FiberHandle<A, E>): Effect.Effect<void> =>
    FiberHandle.set(self, fiber.fiber, options);

export const flickGet = <A, E>(self: FiberHandle.FiberHandle<A, E>) => Effect.map(FiberHandle.get(self), flickFiberOption);

export const flickRun =
  (options: RunOptions) =>
  <A, E, R>(task: Effect.Effect<A, E, R>) =>
  (self: FiberHandle.FiberHandle<A, E>) =>
    Effect.map(FiberHandle.run(self, inOwner(self, task), options), flickFiber);
