// Lexical owners. Each `scope` has one: a child group of the fibers spawned
// into it, the foreign work it waits for, and a native scope of resource
// finalizers. Closing it is two-phase: admission ends, children are
// interrupted and each one awaited (supervisors included), pending foreign
// promises settle, and only then resource finalizers run, last first.
import { Cause, Context, Effect, Exit, Fiber, Scope } from "effect";

import { Failure } from "./cause.js";

/** The owner of the innermost scope; the entry runner's root owner outside one. */
export const CurrentOwner = Context.Reference("@flick-lang/runtime/CurrentOwner", {
  defaultValue: () => undefined,
});

export class Owner {
  constructor(resources) {
    /** Native scope of the resource finalizers. */
    this.resources = resources;
    this.open = true;
    /** Running children, then the fibers that report their failures. */
    this.children = new Set();
    this.watchers = new Set();
    /** Foreign promises still running. */
    this.pending = new Set();
    /** Effects run first when closing begins: listeners stop taking events. */
    this.closing = new Set();
  }

  track(promise) {
    this.pending.add(promise);
    const done = () => this.pending.delete(promise);
    promise.then(done, done);
  }
}

/** Defects of supervisors: the entry runner reports them and fails. */
const reported = [];
export const reportDefect = (cause) => {
  reported.push(cause);
};
export const reportedDefects = () => reported;

const settle = async (owner) => {
  while (owner.pending.size > 0) {
    await Promise.allSettled([...owner.pending]);
  }
};

/** The two-phase close of `owner` after its body ended with `exit`. */
export const close = (owner, exit) =>
  Effect.gen(function* () {
    owner.open = false;
    for (const hook of owner.closing) yield* Effect.exit(hook());
    yield* Fiber.interruptAll([...owner.children]);
    while (owner.watchers.size > 0) {
      yield* Fiber.await([...owner.watchers][0]);
    }
    yield* Effect.promise(() => settle(owner));
    const closed = yield* Effect.exit(Scope.close(owner.resources, exit));
    if (Exit.isSuccess(closed)) return exit;
    return Exit.isSuccess(exit)
      ? Exit.failCause(closed.cause)
      : Exit.failCause(Cause.combine(exit.cause, closed.cause));
  });

const resume = (exit) => (Exit.isSuccess(exit) ? Effect.succeed(exit.value) : Effect.failCause(exit.cause));

/** Runs `effect` with `owner` as the owner of what it acquires and spawns. */
const inside = (owner, effect) =>
  effect.pipe(
    Effect.provideService(Scope.Scope, owner.resources),
    Effect.provideService(CurrentOwner, owner),
  );

/**
 * Runs `body(owner, outer)` as the owner of a fresh scope, then closes it;
 * `outer` is the owner around it.
 */
export const withOwner = (body) =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const outer = yield* Effect.service(CurrentOwner);
      const resources = yield* Scope.make();
      const owner = new Owner(resources);
      const exit = yield* Effect.exit(restore(inside(owner, body(owner, outer))));
      return yield* resume(yield* close(owner, exit));
    }),
  );

/**
 * Runs `body(owner)` as the owner of a fresh scope and closes it; the exit
 * of the body and that of the cleanup apart, for the test runner.
 */
export const ownedExits = (body) =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const resources = yield* Scope.make();
      const owner = new Owner(resources);
      const exit = yield* Effect.exit(restore(inside(owner, body(owner))));
      const cleanup = yield* close(owner, Exit.void);
      return { exit, cleanup };
    }),
  );

/** `scope [s] { body }`: `body` receives the owner `s` and the one around it. */
export const scope = (body) => withOwner(body);

/**
 * A call that acquires into the owner of a region the compiler chose
 * lexically: an outer `scope`'s, not the innermost one's.
 */
export const within = (owner, effect) => inside(owner, effect);

/** `Fiber e a`: a child spawned into an owner. */
export class FlickFiber {
  constructor(fiber) {
    this.fiber = fiber;
    Object.freeze(this);
  }
}

/**
 * `Fiber.spawn owner supervisor task`: runs `task` as a child of `owner`.
 * A failure of the child, its interruption by a closing owner included,
 * goes to `supervisor`; a defect of the supervisor goes to the root
 * reporter. The child acquires into its owner's scope.
 */
export const fiberSpawn = (owner, supervisor, task) =>
  Effect.gen(function* () {
    if (!owner.open) {
      return yield* Effect.die(new Error("the owner is closing: it admits no new children"));
    }
    const child = yield* Effect.forkDetach(inside(owner, task));
    owner.children.add(child);
    const watcher = yield* Effect.forkDetach(
      Effect.uninterruptible(
        Effect.gen(function* () {
          const exit = yield* Fiber.await(child);
          owner.children.delete(child);
          if (Exit.isFailure(exit)) {
            const shutdown = !owner.open && Cause.hasInterruptsOnly(exit.cause);
            const supervised = yield* Effect.exit(supervisor(new Failure(exit.cause, shutdown)));
            if (Exit.isFailure(supervised)) reportDefect(supervised.cause);
          }
        }),
      ),
    );
    owner.watchers.add(watcher);
    watcher.addObserver(() => owner.watchers.delete(watcher));
    return new FlickFiber(child);
  });

/** `Fiber.join fiber`: its result, or its failure with the original cause. */
export const fiberJoin = (fiber) => Fiber.join(fiber.fiber);

/**
 * A foreign `async` call: `start(signal)` creates the promise when the call
 * runs. An interruption aborts `signal`; the promise may still ignore it, so
 * the innermost owner waits for it to settle before releasing resources. A
 * throw or rejection is a defect, or with `mapping` (`catch`) a typed error.
 */
export const foreignAsync = (start, mapping) =>
  Effect.flatMap(Effect.service(CurrentOwner), (owner) =>
    Effect.callback((resume, signal) => {
      const fail = (error) => resume(mapping ? Effect.fail(mapping(error)) : Effect.die(error));
      let promise;
      try {
        promise = Promise.resolve(start(signal));
      } catch (error) {
        fail(error);
        return;
      }
      owner?.track(promise);
      promise.then((value) => resume(Effect.succeed(value)), fail);
    }),
  );

/** A foreign `sync` call with `catch`: a throw becomes the mapped error. */
export const foreignSync = (run, mapping) =>
  Effect.try({ try: run, catch: (error) => mapping(error) });

/** A foreign `effect` with `catch`: a throw of the factory becomes the mapped error. */
export const foreignEffect = (make, mapping) =>
  Effect.suspend(() => {
    try {
      return make();
    } catch (error) {
      return Effect.fail(mapping(error));
    }
  });

/**
 * A resource released when the owner the call runs in closes (the compiler
 * picks it by region); `release` has no typed errors.
 */
export const acquireRelease = (acquire, release) =>
  Effect.acquireRelease(acquire, (resource) => release(resource));

/** `bracket`: `release` runs after `use` and may fail with a typed error. */
export const bracket = (acquire, use, release) =>
  Effect.acquireUseRelease(acquire, use, (resource) => release(resource));
