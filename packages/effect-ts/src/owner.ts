// Lexical owners. Each `scope` has one: a child group of the fibers spawned
// into it, the foreign work it waits for, and a native scope of resource
// finalizers. Closing it is two-phase: admission ends, children are
// interrupted and each one awaited (supervisors included), pending foreign
// promises settle, and only then resource finalizers run, last first.
import { Cause, Context, Effect, Exit, Fiber, Scope } from "effect";

import { Failure } from "./cause.ts";
import { spawnedChild, traceChild, traceEvent, traceId, tracing } from "./trace.ts";

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
  /** Failures of supervisors: the root's fail the program, a test's fail the test. */
  readonly defects: Array<Cause.Cause<unknown>>;
  /** Its id in the trace, 0 when it is not traced, and the finalizers Flick added to it. */
  trace = 0;
  fins = 0;
  /** Whether its finalizers have started: one added now runs at once. */
  finalizing = false;

  constructor(resources: Scope.Closeable, defects: Array<Cause.Cause<unknown>> = reported) {
    this.resources = resources;
    this.defects = defects;
  }

  track(promise: Promise<unknown>): void {
    this.pending.add(promise);
    const traced = tracing && this.trace !== 0 ? { o: this.trace, t: traceId() } : undefined;
    if (traced) traceEvent({ e: "track", ...traced });
    const done = (): void => {
      this.pending.delete(promise);
      if (traced) traceEvent({ e: "settle", ...traced });
    };
    promise.then(done, done);
  }

  /** Writes a phase of its close to the trace. */
  phase(phase: string): void {
    if (tracing && this.trace !== 0) traceEvent({ e: "phase", o: this.trace, c: phase });
  }
}

/** Defects of supervisors under the root: the entry runner reports them and fails. */
const reported: Array<Cause.Cause<unknown>> = [];
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
    owner.phase("C0");
    let cause: Cause.Cause<unknown> = Exit.isFailure(exit) ? exit.cause : Cause.empty;
    const ran = new Set<() => Effect.Effect<unknown, unknown>>();
    const hooks = Effect.gen(function* () {
      for (const hook of owner.closing) {
        if (ran.has(hook)) continue;
        ran.add(hook);
        const closed = yield* Effect.exit(Effect.suspend(hook));
        if (Exit.isFailure(closed)) cause = Cause.combine(cause, closed.cause);
      }
    });
    owner.phase("C1");
    yield* hooks;
    owner.phase("C2");
    if (tracing && owner.trace !== 0) {
      for (const child of owner.children) {
        // the children of callbacks have no watcher and are not traced
        const f = spawnedChild(child);
        if (f !== undefined) traceEvent({ e: "interrupt", o: owner.trace, f });
      }
    }
    yield* Fiber.interruptAll([...owner.children]);
    for (let next = first(owner.watchers); next !== undefined; next = first(owner.watchers)) {
      yield* Fiber.await(next);
    }
    owner.phase("C3");
    // a supervisor may have made a fiber group while the children ended
    yield* hooks;
    // a settling foreign call may add a hook, and a hook may start one: repeat until neither (`docs/formal/runtime.md#a-hook-added-during-c4-does-not-run`)
    for (;;) {
      yield* Effect.promise(() => settle(owner));
      if ([...owner.closing].every((hook) => ran.has(hook))) break;
      yield* hooks;
    }
    owner.phase("C4");
    owner.finalizing = true;
    owner.phase("C5");
    const closed = yield* Effect.exit(Scope.close(owner.resources, exit));
    owner.phase("C6");
    if (Exit.isFailure(closed)) cause = Cause.combine(cause, closed.cause);
    return cause.reasons.length === 0 ? exit : Exit.failCause(cause);
  });

const first = <A>(set: ReadonlySet<A>): A | undefined => set.values().next().value;

const resume = <A, E>(exit: Exit.Exit<A, E>): Effect.Effect<A, E> =>
  Exit.isSuccess(exit) ? Effect.succeed(exit.value) : Effect.failCause(exit.cause);

/** Runs `effect` with `owner` as the owner of what it acquires and spawns. */
export const inside = <A, E, R>(owner: Owner, effect: Effect.Effect<A, E, R>): Effect.Effect<A, E, Exclude<R, Scope.Scope>> =>
  effect.pipe(Effect.provideService(Scope.Scope, owner.resources), Effect.provideService(CurrentOwner, owner));

/**
 * Runs `body(owner, outer)` as the owner of a fresh scope, then closes it;
 * `outer` is the owner around it. An owner of a `kind` (main, scope,
 * provide) is traced.
 */
export const withOwner = <A, E, R>(
  body: (owner: Owner, outer: Owner | undefined) => Effect.Effect<A, E, R>,
  kind?: string,
): Effect.Effect<A, unknown, Exclude<R, Scope.Scope>> =>
  Effect.uninterruptibleMask((restore) =>
    Effect.gen(function* () {
      const outer = yield* Effect.service(CurrentOwner);
      const resources = yield* Scope.make();
      const owner = new Owner(resources, outer?.defects);
      if (tracing && kind !== undefined) {
        owner.trace = traceId();
        traceEvent({ e: "open", o: owner.trace, p: outer?.trace ?? 0, k: kind });
      }
      const exit = yield* Effect.exit(restore(inside(owner, body(owner, outer))));
      return yield* resume(yield* close(owner, exit));
    }),
  );

/** The exit of a test body and that of its cleanup, apart, and the failures of its supervisors. */
export interface OwnedExits<A, E> {
  readonly exit: Exit.Exit<A, E>;
  readonly cleanup: Exit.Exit<unknown, unknown>;
  readonly defects: ReadonlyArray<Cause.Cause<unknown>>;
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
      const owner = new Owner(resources, []);
      const exit = yield* Effect.exit(restore(inside(owner, body(owner))));
      const cleanup = yield* close(owner, Exit.void);
      return { exit, cleanup, defects: owner.defects };
    }),
  );

/** `scope [s] { body }`: `body` receives the owner `s` and the one around it. */
export const scope = <A, E, R>(
  body: (owner: Owner, outer: Owner | undefined) => Effect.Effect<A, E, R>,
): Effect.Effect<A, unknown, Exclude<R, Scope.Scope>> => withOwner(body, "scope");

/** `Scope.run f`, which `scope s { body }` stands for. */
export const scopeRun = <A, E, R>(
  f: (owner: Owner) => Effect.Effect<A, E, R>,
): Effect.Effect<A, unknown, Exclude<R, Scope.Scope>> => withOwner((owner) => f(owner), "scope");

/** `Scope.current ()`: the owner around, which a nested `scope` may acquire into. */
export const scopeCurrent = (): Effect.Effect<Owner | undefined> => Effect.service(CurrentOwner);

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
 * reporter. The child and the supervisor acquire into its owner's scope.
 * Uninterruptible, so that no child is left without its watcher.
 */
export const fiberSpawn = <A, E, R, R2>(
  owner: Owner,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R2>,
  task: Effect.Effect<A, E, R>,
): Effect.Effect<FlickFiber<A, E>, never, Exclude<R, Scope.Scope> | R2> =>
  Effect.uninterruptible(Effect.gen(function* () {
    const traced = tracing && owner.trace !== 0;
    if (!owner.open) {
      if (traced) traceEvent({ e: "refuse", o: owner.trace });
      return yield* Effect.die(new Error("the owner is closing: it admits no new children"));
    }
    const child = yield* Effect.forkDetach(inside(owner, task));
    owner.children.add(child);
    const watcher = yield* Effect.forkDetach(
      Effect.uninterruptible(
        Effect.gen(function* () {
          const exit = yield* Fiber.await(child);
          owner.children.delete(child);
          if (traced) traceEvent({ e: "end", o: owner.trace, f: traceChild(child), x: traceExit(exit) });
          if (Exit.isFailure(exit)) {
            const shutdown = !owner.open && Cause.hasInterruptsOnly(exit.cause);
            if (traced) traceEvent({ e: "sup", o: owner.trace, f: traceChild(child) });
            const supervised = yield* Effect.exit(inside(owner, supervisor(new Failure(exit.cause, shutdown))));
            if (Exit.isFailure(supervised)) owner.defects.push(supervised.cause);
          }
          if (traced) traceEvent({ e: "watched", o: owner.trace, f: traceChild(child) });
        }),
      ),
    );
    owner.watchers.add(watcher);
    watcher.addObserver(() => owner.watchers.delete(watcher));
    if (traced) traceEvent({ e: "spawn", o: owner.trace, f: traceChild(child) });
    return new FlickFiber(child);
  }));

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
): Effect.Effect<A, E, R | R2 | Scope.Scope> =>
  tracing
    ? Effect.flatMap(Effect.scope, (resources) =>
        Effect.map(
          Effect.acquireRelease(
            Effect.flatMap(acquire, (resource) => Effect.map(traceFinalizer(resources), (ran) => ({ resource, ran }))),
            ({ resource, ran }) => Effect.andThen(ran, release(resource)),
          ),
          ({ resource }) => resource,
        ),
      )
    : Effect.acquireRelease(acquire, (resource) => release(resource));

/** How a fiber ended, as the trace names it. */
export const traceExit = (exit: Exit.Exit<unknown, unknown>): string => {
  if (Exit.isSuccess(exit)) return "ok";
  return Cause.hasFails(exit.cause) || Cause.hasDies(exit.cause) ? "fail" : "int";
};

/**
 * Writes a finalizer Flick adds to `resources` to the trace, when they are
 * a traced owner's: the effect to run as it starts writes that it ran.
 */
export const traceFinalizer = (resources: Scope.Scope): Effect.Effect<Effect.Effect<void>> =>
  Effect.map(Effect.service(CurrentOwner), (owner) => {
    if (!tracing || owner === undefined || owner.trace === 0 || owner.resources !== resources) return Effect.void;
    const event = { o: owner.trace, i: ++owner.fins };
    traceEvent({ e: owner.finalizing ? "now" : "fin", ...event });
    return Effect.sync(() => traceEvent({ e: "run", ...event }));
  });

/** `bracket`: `release` runs after `use` and may fail with a typed error. */
export const bracket = <A, E, R, B, E2, R2, E3, R3>(
  acquire: Effect.Effect<A, E, R>,
  use: (resource: A) => Effect.Effect<B, E2, R2>,
  release: (resource: A) => Effect.Effect<unknown, E3, R3>,
): Effect.Effect<B, E | E2 | E3, R | R2 | R3> => Effect.acquireUseRelease(acquire, use, (resource) => release(resource));
