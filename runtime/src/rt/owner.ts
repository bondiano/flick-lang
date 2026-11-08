// Lexical owners (docs/formal/runtime.md §3.3). Each `scope` has one: its
// children, the foreign calls it waits for, closing hooks and a stack of
// finalizers. Closing it is a fixed sequence of phases: admission ends (C0),
// hooks run (C1), children are asked to stop (C2) and each is awaited with its
// supervisor (C3), foreign calls settle (C4), finalizers run, last added first
// (C5), and the causes join (C6).
import {
  type Cause,
  type Exit,
  Raise,
  raise,
  combine,
  concat,
  elect,
  empty,
  fail,
  failed,
  hasDies,
  hasFails,
  hasInterruptsOnly,
  succeeded,
} from "./cause.ts";
import {
  type Ctx,
  type Fiber,
  type Run,
  Suspend,
  Under,
  asExit,
  atMask,
  awaitFiber,
  currentCtx,
  currentFiber,
  exitOf,
  fork,
  owned,
  raiseOf,
  uninterruptible,
  underSync,
} from "./fiber.ts";
import { traceEvent, traceId, tracing } from "./trace.ts";
import { Failure } from "./outcome.ts";

/** A finalizer: the evidence it was added with, what it runs, and its place in the trace. */
interface Finalizer {
  readonly ctx: Ctx;
  readonly run: (exit: Exit) => unknown;
  readonly place: number;
}

/** Defects of supervisors under the root: the entry runner reports them and fails. */
const reported: Array<Cause> = [];
export const reportedDefects = (): ReadonlyArray<Cause> => reported;

export class Owner {
  /** C0 not yet: it admits children. */
  open = true;
  readonly children = new Set<Fiber>();
  readonly watchers = new Set<Fiber>();
  /** Foreign promises not yet settled. */
  readonly pending = new Set<Promise<unknown>>();
  /** Hooks of C1, in order of addition. */
  readonly closing = new Set<() => unknown>();
  /** Failures of supervisors: the root's fail the program, a test's fail the test. */
  readonly defects: Array<Cause>;
  readonly finalizers: Array<Finalizer> = [];
  /** Its id in the trace, 0 when not traced, and the finalizers Flick added to it. */
  trace = 0;
  fins = 0;
  /** C5 has started: a finalizer added now runs at once, with this exit. */
  finalizing = false;
  exit: Exit | undefined = undefined;

  constructor(defects: Array<Cause> = reported) {
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

/** How a fiber ended, as the trace names it. */
export const traceExit = (exit: Exit): string => {
  if (exit.cause === undefined) return "ok";
  return hasFails(exit.cause) || hasDies(exit.cause) ? "fail" : "int";
};

const settle = async (owner: Owner): Promise<void> => {
  while (owner.pending.size > 0) {
    // a settling promise may start more foreign work: wait again until none is left
    // oxlint-disable-next-line no-await-in-loop -- each round waits for what the previous one started
    await Promise.allSettled(owner.pending);
  }
};

const promised = (promise: Promise<unknown>): Suspend =>
  new Suspend((resolve, reject) => {
    promise.then(resolve, reject);
  });

const first = <A>(set: ReadonlySet<A>): A | undefined => set.values().next().value;

/** Runs a finalizer, masked, with the evidence it was added with: what it failed with. */
function* runFinalizer(finalizer: Finalizer, owner: Owner, exit: Exit): Run {
  if (finalizer.place !== 0) traceEvent({ e: "run", o: owner.trace, i: finalizer.place });
  const ended = yield uninterruptible(() => new Under(finalizer.ctx, () => finalizer.run(exit)));
  return ended;
}

function* closeSteps(owner: Owner, exit: Exit): Run {
  owner.open = false;
  owner.phase("C0");
  let cause: Cause = exit.cause ?? empty;
  const ran = new Set<() => unknown>();
  function* hooks(): Run {
    for (const hook of owner.closing) {
      if (ran.has(hook)) continue;
      ran.add(hook);
      const closed = asExit(yield exitOf(hook));
      if (closed.cause !== undefined) cause = combine(cause, closed.cause);
    }
    return undefined;
  }
  owner.phase("C1");
  yield hooks();
  owner.phase("C2");
  const fiber = currentFiber();
  const children = [...owner.children];
  if (tracing && owner.trace !== 0) {
    for (const child of children) {
      const f = spawned.get(child);
      if (f !== undefined) traceEvent({ e: "interrupt", o: owner.trace, f });
    }
  }
  for (const child of children) child.interrupt(fiber.id);
  for (const child of children) yield awaitFiber(child);
  for (let next = first(owner.watchers); next !== undefined; next = first(owner.watchers)) {
    yield awaitFiber(next);
  }
  owner.phase("C3");
  // a supervisor may have made a fiber group while the children ended
  yield hooks();
  // a settling foreign call may add a hook, and a hook may start one: repeat until neither
  for (;;) {
    // a stage that runs at once, or a fiber it waits for, cannot wait for a promise, and has none to wait for
    if (owner.pending.size > 0 || (fiber.nested === 0 && !underSync())) yield promised(settle(owner));
    if ([...owner.closing].every((hook) => ran.has(hook))) break;
    yield hooks();
  }
  owner.phase("C4");
  owner.finalizing = true;
  owner.exit = exit;
  owner.phase("C5");
  let released: Cause = empty;
  while (owner.finalizers.length > 0) {
    const finalizer = owner.finalizers.pop();
    if (finalizer === undefined) break;
    const ended = asExit(yield exitOf(() => runFinalizer(finalizer, owner, exit)));
    if (ended.cause !== undefined) released = concat(released, ended.cause);
  }
  owner.phase("C6");
  cause = combine(cause, released);
  return cause.reasons.length === 0 ? exit : failed(cause);
}

/** The close of `owner` after its body ended with `exit`: how the region ends. */
export const close = (owner: Owner, exit: Exit): Run => closeSteps(owner, exit);

/** Runs `task` with `owner` as the owner of what it acquires and spawns. */
export const inside = (owner: Owner, task: () => unknown): Under => new Under(owned(currentCtx(), owner), task);

/**
 * Runs `body(owner, outer)` as the owner of a fresh scope, then closes it. The
 * body is interruptible, as the caller was; the close is not. An owner of a
 * `kind` (main, scope, provide) is traced.
 */
export function* withOwner(body: (owner: Owner, outer: Owner | undefined) => unknown, kind?: string): Run {
  const fiber = currentFiber();
  const outer = currentCtx().owner;
  const depth = fiber.mask;
  fiber.mask = depth + 1;
  try {
    const owner = new Owner(outer?.defects);
    if (tracing && kind !== undefined) {
      owner.trace = traceId();
      traceEvent({ e: "open", o: owner.trace, p: outer?.trace ?? 0, k: kind });
    }
    const ctx = owned(currentCtx(), owner);
    const exit = asExit(yield exitOf(() => atMask(depth, () => new Under(ctx, () => body(owner, outer)))));
    return raiseOf(asExit(yield close(owner, exit)));
  } finally {
    fiber.mask = depth;
  }
}

/** The exit of a test body and that of its cleanup, apart, and the failures of its supervisors. */
export interface OwnedExits {
  readonly exit: Exit;
  readonly cleanup: Exit;
  readonly defects: ReadonlyArray<Cause>;
}

/** Runs `body(owner)` as the owner of a fresh scope and closes it, for the test runner. */
export function* ownedExits(body: (owner: Owner) => unknown): Run {
  const fiber = currentFiber();
  const depth = fiber.mask;
  fiber.mask = depth + 1;
  try {
    const owner = new Owner([]);
    const ctx = owned(currentCtx(), owner);
    const exit = asExit(yield exitOf(() => atMask(depth, () => new Under(ctx, () => body(owner)))));
    const cleanup = asExit(yield close(owner, succeeded(undefined)));
    const out: OwnedExits = { exit, cleanup, defects: owner.defects };
    return out;
  } finally {
    fiber.mask = depth;
  }
}

/** `Scope.run f`, which `scope s { body }` stands for. */
export const scopeRun = (f: (owner: Owner) => unknown): Run => withOwner((owner) => f(owner), "scope");

/** `Scope.current ()`: the owner around, which a nested `scope` may acquire into. */
export const scopeCurrent = (): Owner | undefined => currentCtx().owner;

/** A call that acquires into the owner of a region the compiler chose: an outer `scope`'s. */
export const within = (owner: Owner, call: () => unknown): Under => inside(owner, call);

/**
 * A finalizer on the stack of `owner`, or run at once when its finalizers have
 * started. The trace has what Flick adds; `silent` is one of the runtime's own.
 */
export const addTo = (owner: Owner | undefined, run: (exit: Exit) => unknown, silent = false): unknown => {
  if (owner === undefined) return undefined;
  const traced = !silent && tracing && owner.trace !== 0;
  const place = traced ? ++owner.fins : 0;
  const finalizer: Finalizer = { ctx: currentCtx(), run, place };
  if (owner.finalizing) {
    if (traced) traceEvent({ e: "now", o: owner.trace, i: place });
    return runNow(finalizer, owner);
  }
  if (traced) traceEvent({ e: "fin", o: owner.trace, i: place });
  owner.finalizers.push(finalizer);
  return undefined;
};

function* runNow(finalizer: Finalizer, owner: Owner): Run {
  const exit = owner.exit ?? succeeded(undefined);
  return yield runFinalizer(finalizer, owner, exit);
}

/** `Scope.add_finalizer`: into the owner of the call. */
export const addFinalizer = (finalizer: () => unknown): unknown => addTo(currentCtx().owner, () => finalizer());

/** A finalizer that sees how the owner's body ended. */
export const addFinalizerExit = (finalizer: (exit: Exit) => unknown): unknown => addTo(currentCtx().owner, finalizer);

/** A resource released when the owner the call runs in closes. */
export function* acquireRelease(acquire: () => unknown, release: (resource: unknown) => unknown): Run {
  const fiber = currentFiber();
  fiber.mask++;
  try {
    const resource: unknown = yield acquire();
    yield addTo(currentCtx().owner, () => release(resource));
    return resource;
  } finally {
    fiber.mask--;
  }
}

/** `bracket`: `release` runs after `use`, and may fail with a typed error. */
export function* bracket(acquire: () => unknown, use: (resource: unknown) => unknown, release: (resource: unknown) => unknown): Run {
  const fiber = currentFiber();
  const depth = fiber.mask;
  fiber.mask = depth + 1;
  try {
    const resource: unknown = yield acquire();
    const used = asExit(yield exitOf(() => atMask(depth, () => use(resource))));
    const released = asExit(yield exitOf(() => release(resource)));
    return raiseOf(finalize(used, released));
  } finally {
    fiber.mask = depth;
  }
}

/** What ended with `exit`, then a finalizer that ended with `fin`: a failure of both elects its defects. */
export const finalize = (exit: Exit, fin: Exit): Exit => {
  if (fin.cause === undefined) return exit;
  if (exit.cause === undefined) return fin.cause.reasons.length === 0 ? exit : failed(fin.cause);
  return failed(elect(combine(exit.cause, fin.cause)));
};

/** `Fiber e a`: a child spawned into an owner. */
export class FlickFiber {
  readonly fiber: Fiber;
  constructor(fiber: Fiber) {
    this.fiber = fiber;
    Object.freeze(this);
  }
}

const spawned = /*#__PURE__*/ new WeakMap<Fiber, number>();

/**
 * `Fiber.spawn owner supervisor task`: runs `task` as a child of `owner`. A
 * failure of the child, its interruption by a closing owner included, goes to
 * `supervisor`; a failure of the supervisor goes to the root. The child and
 * the supervisor acquire into `owner`. Nothing here yields, so nothing splits
 * registering the child from registering its watcher.
 */
export const fiberSpawn = (owner: Owner, supervisor: (failure: Failure) => unknown, task: () => unknown): FlickFiber => {
  const traced = tracing && owner.trace !== 0;
  if (!owner.open) {
    if (traced) traceEvent({ e: "refuse", o: owner.trace });
    throw new Error("the owner is closing: it admits no new children");
  }
  const ctx = owned(currentCtx(), owner);
  const child = fork(task, ctx);
  owner.children.add(child);
  const id = traced ? traceId() : 0;
  if (traced) spawned.set(child, id);
  const watcher = fork(() => watch(owner, child, supervisor, id), ctx, 1);
  owner.watchers.add(watcher);
  watcher.observe(() => owner.watchers.delete(watcher));
  if (traced) traceEvent({ e: "spawn", o: owner.trace, f: id });
  return new FlickFiber(child);
};

function* watch(owner: Owner, child: Fiber, supervisor: (failure: Failure) => unknown, id: number): Run {
  const exit = asExit(yield awaitFiber(child));
  owner.children.delete(child);
  const traced = id !== 0;
  if (traced) traceEvent({ e: "end", o: owner.trace, f: id, x: traceExit(exit) });
  if (exit.cause !== undefined) {
    const shutdown = !owner.open && hasInterruptsOnly(exit.cause);
    if (traced) traceEvent({ e: "sup", o: owner.trace, f: id });
    const failure = new Failure(exit.cause, shutdown);
    const supervised = asExit(yield exitOf(() => supervisor(failure)));
    if (supervised.cause !== undefined) owner.defects.push(supervised.cause);
  }
  if (traced) traceEvent({ e: "watched", o: owner.trace, f: id });
  return undefined;
}

/** `Fiber.join fiber`: its result, or its failure with the original cause. */
export function* fiberJoin(fiber: FlickFiber): Run {
  return raiseOf(asExit(yield awaitFiber(fiber.fiber)));
}

/**
 * A foreign `async` call: `start(signal)` makes the promise when the call runs.
 * An interruption aborts `signal`; the promise may still ignore it, so the
 * innermost owner waits for it to settle before it releases anything. A throw
 * or rejection is a defect, or with `mapping` (`catch`) a typed error.
 */
export const foreignAsync = (start: (signal: AbortSignal) => unknown, mapping?: (error: unknown) => unknown): Suspend => {
  const owner = currentCtx().owner;
  return new Suspend((resolve, reject) => {
    const failure = (error: unknown): void => reject(mapping ? new Raise(fail(mapping(error))) : error);
    const controller = new AbortController();
    let promise: Promise<unknown>;
    try {
      promise = Promise.resolve(start(controller.signal));
    } catch (error) {
      failure(error);
      return undefined;
    }
    owner?.track(promise);
    promise.then(resolve, failure);
    return () => controller.abort();
  });
};


/** A foreign `sync` call with `catch`: a throw becomes the mapped error. */
export const foreignSync = (run: () => unknown, mapping: (error: unknown) => unknown): unknown => {
  try {
    return run();
  } catch (error) {
    return raise(fail(mapping(error)));
  }
};
