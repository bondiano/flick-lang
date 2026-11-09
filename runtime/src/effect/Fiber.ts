// Effect's `Fiber` for the bindings of the standard library, and the
// adapters over Flick's `Fiber e a`: a child spawned into an owner.
import { Effect, Fiber, Option } from "effect";

import type { FlickExit } from "../cause.ts";
import type { FlickFiber } from "../owner.ts";
import { flickExit } from "../stdlib/outcome.ts";

export * from "effect/Fiber";

const natives = <A, E>(fibers: ReadonlyArray<FlickFiber<A, E>>): Array<Fiber.Fiber<A, E>> => fibers.map((f) => f.fiber);

export const flickAwait = <A, E>(fiber: FlickFiber<A, E>): Effect.Effect<FlickExit<A>> =>
  Effect.map(Fiber.await(fiber.fiber), flickExit);
export const flickAwaitAll = <A, E>(fibers: ReadonlyArray<FlickFiber<A, E>>): Effect.Effect<Array<FlickExit<A>>> =>
  Effect.map(Fiber.awaitAll(natives(fibers)), (exits) => exits.map(flickExit));
export const flickJoinAll = <A, E>(fibers: ReadonlyArray<FlickFiber<A, E>>): Effect.Effect<ReadonlyArray<A>, E> =>
  Fiber.joinAll(natives(fibers));
export const flickInterrupt = <A, E>(fiber: FlickFiber<A, E>): Effect.Effect<void> => Fiber.interrupt(fiber.fiber);
export const flickInterruptAs =
  (fiberId: Option.Option<number>) =>
  <A, E>(fiber: FlickFiber<A, E>): Effect.Effect<void> =>
    Fiber.interruptAs(fiber.fiber, Option.getOrUndefined(fiberId));
export const flickInterruptAll = <A, E>(fibers: ReadonlyArray<FlickFiber<A, E>>): Effect.Effect<void> =>
  Fiber.interruptAll(natives(fibers));
export const flickInterruptAllAs =
  (fiberId: number) =>
  <A, E>(fibers: ReadonlyArray<FlickFiber<A, E>>): Effect.Effect<void> =>
    Fiber.interruptAllAs(natives(fibers), fiberId);
