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

/**
 * Runs `body(owner)` as the owner of a fresh scope, then closes it.
 *
 * shortcut: an acquisition finds its owner as the innermost scope in the
 * context, not the region its `Scope s` names; pass owners explicitly once
 * resource APIs can need an outer named scope.
 */
export const withOwner = (body) =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const resources = yield* Scope.make();
      const owner = new Owner(resources);
      const exit = yield* Effect.exit(
        restore(
          body(owner).pipe(
            Effect.provideService(Scope.Scope, resources),
            Effect.provideService(CurrentOwner, owner),
          ),
        ),
      );
      return yield* resume(yield* close(owner, exit));
    }),
  );

/** `scope [s] { body }`: `body` receives the owner `s`. */
export const scope = (body) => withOwner(body);

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
    const child = yield* Effect.forkDetach(
      task.pipe(
        Effect.provideService(Scope.Scope, owner.resources),
        Effect.provideService(CurrentOwner, owner),
      ),
    );
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
 * A foreign `async` call. Its promise may ignore interruption, so the
 * innermost owner waits for it to settle before releasing resources.
 */
export const foreignAsync = (start) =>
  Effect.flatMap(Effect.service(CurrentOwner), (owner) =>
    Effect.callback((resume) => {
      let promise;
      try {
        promise = Promise.resolve(start());
      } catch (error) {
        resume(Effect.die(error));
        return;
      }
      owner?.track(promise);
      promise.then(
        (value) => resume(Effect.succeed(value)),
        (error) => resume(Effect.die(error)),
      );
    }),
  );

/** A resource released when its owner closes; `release` has no typed errors. */
export const acquireRelease = (acquire, release) =>
  Effect.acquireRelease(acquire, (resource) => release(resource));

/** `bracket`: `release` runs after `use` and may fail with a typed error. */
export const bracket = (acquire, use, release) =>
  Effect.acquireUseRelease(acquire, use, (resource) => release(resource));
