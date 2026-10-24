// Effect's `FiberHandle`, `FiberMap` and `FiberSet` as owners of the
// region they are made in: each remembers the owner of its `scope`, and a
// task it runs acquires into that owner, as a `Fiber.spawn` child does.
import { Effect, type Fiber, Option, type Scope } from "effect";

import { CurrentOwner, FlickFiber, type Owner, within } from "../owner.ts";

const owners = /*#__PURE__*/ new WeakMap<object, Owner>();

/** Makes the collection and remembers the owner the call acquires into. */
export const owned = <T extends object>(make: Effect.Effect<T, never, Scope.Scope>): Effect.Effect<T, never, Scope.Scope> =>
  Effect.flatMap(Effect.service(CurrentOwner), (owner) =>
    Effect.map(make, (value) => {
      if (owner !== undefined) owners.set(value, owner);
      return value;
    }),
  );

/** `task` acquiring into the owner of `collection`. */
export const inOwner = <A, E, R>(collection: object, task: Effect.Effect<A, E, R>): Effect.Effect<A, E, Exclude<R, Scope.Scope>> => {
  const owner = owners.get(collection);
  // without an owner (made outside every scope) the task keeps the scope of its fiber
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler provides `Scope` lexically
  return owner === undefined ? (task as Effect.Effect<A, E, Exclude<R, Scope.Scope>>) : within(owner, task);
};

/** A native child as a Flick `Fiber`. */
export const flickFiber = <A, E>(fiber: Fiber.Fiber<A, E>): FlickFiber<A, E> => new FlickFiber(fiber);

/** The Flick `Fiber` of an `Option` of a native one. */
export const flickFiberOption = <A, E>(fiber: Option.Option<Fiber.Fiber<A, E>>): Option.Option<FlickFiber<A, E>> =>
  Option.map(fiber, flickFiber);
