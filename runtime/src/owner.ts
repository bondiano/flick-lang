// Lexical owners. Each `scope` has one: a child group of the fibers spawned
// into it, the foreign work it waits for, and a native scope of resource
// finalizers. Closing it is two-phase: admission ends, children are
// interrupted and each one awaited (supervisors included), pending foreign
// promises settle, and only then resource finalizers run, last first.
import { Cause, Context, Effect, Exit, Fiber, Scope } from "effect";

import { Failure } from "./cause.ts";

/** The owner of the innermost scope; the entry runner's root owner outside one. */
export const CurrentOwner = /*#__PURE__*/ Context.Reference<Owner | undefined>("@flick-lang/runtime/CurrentOwner", {
  defaultValue: () => undefined,
});

export class Owner {
  /** Native scope of the resource finalizers. */
  readonly resources: Scope.Closeable;
  open = true;
  /** Running children, then the fibers that report their failures. */
  readonly children = new Set<Fiber.Fiber<unknown, unknown>>();
  readonly watchers = new Set<Fiber.Fiber<unknown, unknown>>();
  /** Foreign promises still running. */
  readonly pending = new Set<Promise<unknown>>();
  /** Effects run first when closing begins: listeners stop taking events. */
  readonly closing = new Set<() => Effect.Effect<unknown, unknown>>();

  constructor(resources: Scope.Closeable) {
    this.resources = resources;
  }

  track(promise: Promise<unknown>): void {
    this.pending.add(promise);
    const done = (): void => void this.pending.delete(promise);
    promise.then(done, done);
  }
}

/** Defects of supervisors: the entry runner reports them and fails. */
const reported: Array<Cause.Cause<unknown>> = [];
export const reportDefect = (cause: Cause.Cause<unknown>): void => {
  reported.push(cause);
};
export const reportedDefects = (): ReadonlyArray<Cause.Cause<unknown>> => reported;

const settle = async (owner: Owner): Promise<void> => {
  while (owner.pending.size > 0) {
    // a settling promise may start more foreign work: wait again until none is left
    // oxlint-disable-next-line no-await-in-loop -- each round waits for what the previous one started
    await Promise.allSettled(owner.pending);
  }
};

/** The two-phase close of `owner` after its body ended with `exit`. */
export const close = <A, E>(owner: Owner, exit: Exit.Exit<A, E>): Effect.Effect<Exit.Exit<A, unknown>> =>
  Effect.gen(function* () {
    owner.open = false;
    for (const hook of owner.closing) yield* Effect.exit(hook());
    yield* Fiber.interruptAll([...owner.children]);
    for (let next = first(owner.watchers); next !== undefined; next = first(owner.watchers)) {
      yield* Fiber.await(next);
    }
    yield* Effect.promise(() => settle(owner));
    const closed = yield* Effect.exit(Scope.close(owner.resources, exit));
    if (Exit.isSuccess(closed)) return exit;
    return Exit.isSuccess(exit)
      ? Exit.failCause(closed.cause)
      : Exit.failCause(Cause.combine(exit.cause, closed.cause));
  });

const first = <A>(set: ReadonlySet<A>): A | undefined => set.values().next().value;

const resume = <A, E>(exit: Exit.Exit<A, E>): Effect.Effect<A, E> =>
  Exit.isSuccess(exit) ? Effect.succeed(exit.value) : Effect.failCause(exit.cause);

/** Runs `effect` with `owner` as the owner of what it acquires and spawns. */
const inside = <A, E, R>(owner: Owner, effect: Effect.Effect<A, E, R>): Effect.Effect<A, E, Exclude<R, Scope.Scope>> =>
  effect.pipe(Effect.provideService(Scope.Scope, owner.resources), Effect.provideService(CurrentOwner, owner));

/**
 * Runs `body(owner, outer)` as the owner of a fresh scope, then closes it;
 * `outer` is the owner around it.
 */
export const withOwner = <A, E, R>(
  body: (owner: Owner, outer: Owner | undefined) => Effect.Effect<A, E, R>,
): Effect.Effect<A, unknown, Exclude<R, Scope.Scope>> =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const outer = yield* Effect.service(CurrentOwner);
      const resources = yield* Scope.make();
      const owner = new Owner(resources);
      const exit = yield* Effect.exit(restore(inside(owner, body(owner, outer))));
      return yield* resume(yield* close(owner, exit));
    }),
  );

/** The exit of a test body and that of its cleanup, apart. */
export interface OwnedExits<A, E> {
  readonly exit: Exit.Exit<A, E>;
  readonly cleanup: Exit.Exit<unknown, unknown>;
}

/**
 * Runs `body(owner)` as the owner of a fresh scope and closes it; the exit
 * of the body and that of the cleanup apart, for the test runner.
 */
export const ownedExits = <A, E, R>(
  body: (owner: Owner) => Effect.Effect<A, E, R>,
): Effect.Effect<OwnedExits<A, E>, never, Exclude<R, Scope.Scope>> =>
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
export const scope = withOwner;

/**
 * A call that acquires into the owner of a region the compiler chose
 * lexically: an outer `scope`'s, not the innermost one's.
 */
export const within = inside;

/** `Fiber e a`: a child spawned into an owner. */
export class FlickFiber<A, E> {
  readonly fiber: Fiber.Fiber<A, E>;
  constructor(fiber: Fiber.Fiber<A, E>) {
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
export const fiberSpawn = <A, E, R, R2>(
  owner: Owner,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R2>,
  task: Effect.Effect<A, E, R>,
): Effect.Effect<FlickFiber<A, E>, never, Exclude<R, Scope.Scope> | R2> =>
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
export const fiberJoin = <A, E>(fiber: FlickFiber<A, E>): Effect.Effect<A, E> => Fiber.join(fiber.fiber);

/**
 * A foreign `async` call: `start(signal)` creates the promise when the call
 * runs. An interruption aborts `signal`; the promise may still ignore it, so
 * the innermost owner waits for it to settle before releasing resources. A
 * throw or rejection is a defect, or with `mapping` (`catch`) a typed error.
 */
export const foreignAsync = <A, E = never>(
  start: (signal: AbortSignal) => A | PromiseLike<A>,
  mapping?: (error: unknown) => E,
): Effect.Effect<A, E> =>
  Effect.flatMap(Effect.service(CurrentOwner), (owner) =>
    Effect.callback<A, E>((done, signal) => {
      const fail = (error: unknown): void => done(mapping ? Effect.fail(mapping(error)) : Effect.die(error));
      let promise: Promise<A>;
      try {
        promise = Promise.resolve(start(signal));
      } catch (error) {
        fail(error);
        return;
      }
      owner?.track(promise);
      promise.then((value) => done(Effect.succeed(value)), fail);
    }),
  );

/** A foreign `sync` call with `catch`: a throw becomes the mapped error. */
export const foreignSync = <A, E>(run: () => A, mapping: (error: unknown) => E): Effect.Effect<A, E> =>
  Effect.try({ try: run, catch: (error) => mapping(error) });

/** A foreign `effect` with `catch`: a throw of the factory becomes the mapped error. */
export const foreignEffect = <A, E, R, E2>(
  make: () => Effect.Effect<A, E, R>,
  mapping: (error: unknown) => E2,
): Effect.Effect<A, E | E2, R> =>
  Effect.suspend((): Effect.Effect<A, E | E2, R> => {
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
export const acquireRelease = <A, E, R, R2>(
  acquire: Effect.Effect<A, E, R>,
  release: (resource: A) => Effect.Effect<unknown, never, R2>,
): Effect.Effect<A, E, R | R2 | Scope.Scope> => Effect.acquireRelease(acquire, (resource) => release(resource));

/** `bracket`: `release` runs after `use` and may fail with a typed error. */
export const bracket = <A, E, R, B, E2, R2, E3, R3>(
  acquire: Effect.Effect<A, E, R>,
  use: (resource: A) => Effect.Effect<B, E2, R2>,
  release: (resource: A) => Effect.Effect<unknown, E3, R3>,
): Effect.Effect<B, E | E2 | E3, R | R2 | R3> => Effect.acquireUseRelease(acquire, use, (resource) => release(resource));
