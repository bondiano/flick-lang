// Effect's `FiberHandle`, `FiberMap` and `FiberSet` as owners of the
// region they are made in: each remembers the owner of its `scope`, and a
// task it runs acquires into that owner, as a `Fiber.spawn` child does.
import { Effect, type Fiber, Option, type Scope } from "effect";

import { CurrentOwner, FlickFiber, type Owner, within } from "../owner.ts";

interface GroupOwner { readonly owner: Owner; open: boolean }
const owners = /*#__PURE__*/ new WeakMap<object, GroupOwner>();

/**
 * Makes the collection and remembers the owner the call acquires into. The
 * owner's closing interrupts its children with `clear` and waits for them
 * before any resource is released, as it does for its own children: the
 * collection's own finalizer would run among the resources, after those
 * acquired later.
 */
export const owned = <T extends object>(
  make: Effect.Effect<T, never, Scope.Scope>,
  clear: (collection: T) => Effect.Effect<void>,
): Effect.Effect<T, never, Scope.Scope> =>
  Effect.flatMap(Effect.service(CurrentOwner), (owner) =>
    Effect.map(make, (value) => {
      if (owner !== undefined) {
        const group = { owner, open: true };
        owners.set(value, group);
        owner.closing.add(() => Effect.suspend(() => {
          group.open = false;
          return clear(value);
        }));
      }
      return value;
    }),
  );

/** `task` acquiring into the owner of `collection`. */
export const inOwner = <A, E, R>(collection: object, task: Effect.Effect<A, E, R>): Effect.Effect<A, E, Exclude<R, Scope.Scope>> => {
  const group = owners.get(collection);
  // without an owner (made outside every scope) the task keeps the scope of its fiber
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler provides `Scope` lexically
  if (group === undefined) return task as Effect.Effect<A, E, Exclude<R, Scope.Scope>>;
  // The group closes at its own C1 hook. A group made during the owner's
  // closing is still open until that hook runs, before resource finalizers.
  return Effect.suspend(() => (group.open ? within(group.owner, task) : Effect.interrupt));
};

/** A native child as a Flick `Fiber`. */
export const flickFiber = <A, E>(fiber: Fiber.Fiber<A, E>): FlickFiber<A, E> => new FlickFiber(fiber);

/** The Flick `Fiber` of an `Option` of a native one. */
export const flickFiberOption = <A, E>(fiber: Option.Option<Fiber.Fiber<A, E>>): Option.Option<FlickFiber<A, E>> =>
  Option.map(fiber, flickFiber);
