// Effect's `FiberMap` for the bindings of the standard library, and the
// adapters that keep its children in the owner of its region.
import { Effect, FiberMap } from "effect";

import type { FlickFiber } from "../owner.ts";
import { flickFiber, flickFiberOption, inOwner, owned } from "../stdlib/fibers.ts";

export * from "effect/FiberMap";

interface SetOptions {
  readonly onlyIfMissing: boolean;
  readonly propagateInterruption: boolean;
}

interface RunOptions extends SetOptions {
  readonly startImmediately: boolean;
}

export const flickMake = <K, A, E>() => owned(FiberMap.make<K, A, E>(), FiberMap.clear);

export const flickSet =
  (options: SetOptions) =>
  <K>(key: K) =>
  <A, E>(fiber: FlickFiber<A, E>) =>
  (self: FiberMap.FiberMap<K, A, E>): Effect.Effect<void> =>
    FiberMap.set(self, key, fiber.fiber, options);

export const flickGet =
  <K>(key: K) =>
  <A, E>(self: FiberMap.FiberMap<K, A, E>) =>
    Effect.map(FiberMap.get(self, key), flickFiberOption);

export const flickRun =
  (options: RunOptions) =>
  <K>(key: K) =>
  <A, E, R>(task: Effect.Effect<A, E, R>) =>
  (self: FiberMap.FiberMap<K, A, E>) =>
    Effect.map(FiberMap.run(self, key, inOwner(self, task), options), flickFiber);
