// Fibers and their driver. Compiled runtime code is a generator: it yields what
// it calls, and the driver keeps the frames on the heap, so a call chain is as
// deep as memory allows and a suspension costs nothing per frame. What a
// generator yields or returns is one of:
//   a generator    a call: a new frame on top (returned: a tail call, the frame replaced);
//   an Instruction what only the driver can do: suspend, change the context, capture;
//   anything else  the value of a call that needed no frame.
// The driver checks for interruption before every step, so a step of the driver
// is an interruption point; a plain function called between two is not.
import { type Cause, Exit, Raise, causeOf, raise, concat, failed, fromReasons, interrupt, isFail, sameReason, succeeded } from "./cause.ts";
import type { Owner } from "./owner.ts";

/** The evidence: an immutable list of what is in effect, innermost first, with the owner there. */
export class Ctx {
  readonly tag: object | null;
  readonly impl: unknown;
  readonly next: Ctx | null;
  readonly owner: Owner | undefined;
  constructor(tag: object | null, impl: unknown, next: Ctx | null, owner: Owner | undefined) {
    this.tag = tag;
    this.impl = impl;
    this.next = next;
    this.owner = owner;
  }
}

const ROOT = /*#__PURE__*/ new Ctx(null, undefined, null, undefined);

let context: Ctx = ROOT;
let current: Fiber | undefined;

/** The evidence in effect now. */
export const currentCtx = (): Ctx => context;

/** The owner of what runs now, the innermost `scope`'s. */
export const currentOwner = (): Owner | undefined => context.owner;

/** The fiber running now; one made for the purpose when code runs outside any. */
export const currentFiber = (): Fiber => {
  current ??= new Fiber();
  return current;
};

/** The implementation of `tag` in the evidence. */
export const lookup = (tag: object): unknown => {
  for (let c: Ctx | null = context; c !== null; c = c.next) {
    if (c.tag === tag) return c.impl;
  }
  return undefined;
};

/** The evidence `ctx` with `impl` for `tag` on top. */
export const extend = (ctx: Ctx, tag: object, impl: unknown): Ctx => new Ctx(tag, impl, ctx, ctx.owner);

/** The evidence `ctx` with `owner` as the owner. */
export const owned = (ctx: Ctx, owner: Owner | undefined): Ctx => new Ctx(null, undefined, ctx, owner);

/** Calls `f` with `ctx` as the evidence. */
export const inCtx = <A>(ctx: Ctx, f: () => A): A => {
  const saved = context;
  context = ctx;
  try {
    return f();
  } finally {
    context = saved;
  }
};

/** A running call. */
export type Run = Generator<unknown, unknown, unknown>;

/** Generator objects inherit from this function's prototype: `instanceof` tells them apart in one check. */
function Generators(): void {}
const generatorFunction: unknown = /*#__PURE__*/ Object.getPrototypeOf(function* () {});
Reflect.set(Generators, "prototype", Reflect.get(Object(generatorFunction), "prototype"));

export const isRun = (value: unknown): value is Run => value instanceof Generators;

/** What only the driver can do. */
export abstract class Instruction {
  /** Moves the fiber on; `true` when it suspended. A throw fails the frame on top. */
  abstract exec(fiber: Fiber): boolean;
}

const NEXT = 0;
const THROW = 1;

/** A frame: the generator, the evidence it runs with, the frame it returns to. */
export class Frame {
  gen: Run;
  readonly ctx: Ctx;
  prev: Frame | null;
  /** The handler whose body starts at this frame. */
  readonly prompt: object | null;
  /** Below it is the JS stack of a Sync stage: nothing captures across it. */
  readonly barrier: boolean;
  constructor(gen: Run, ctx: Ctx, prev: Frame | null, prompt: object | null, barrier: boolean) {
    this.gen = gen;
    this.ctx = ctx;
    this.prev = prev;
    this.prompt = prompt;
    this.barrier = barrier;
  }
}

/** The operations a fiber runs before it gives others a turn. */
const BUDGET = 2048;

let fiberIds = 0;

export class Fiber {
  readonly id: number = ++fiberIds;
  top: Frame | null = null;
  mode: 0 | 1 = NEXT;
  value: unknown = undefined;
  /** The mask depth: interruptible at zero. */
  mask = 0;
  /** Who asked it to stop, 0 for nobody; sticky. */
  requested = 0;
  /** While it is suspended, what stops the wait, and whether it waits at all. */
  suspended = false;
  cancel: (() => void) | undefined = undefined;
  exit: Exit | undefined = undefined;
  started = false;
  /** How many Sync stages hold it on the JS stack: it wakes in the one on top. */
  nested = 0;
  private observers: Array<(exit: Exit) => void> = [];
  private ops = 0;

  /** Runs `f` when it ends, at once if it has; removes it with the function returned. */
  observe(f: (exit: Exit) => void): () => void {
    if (this.exit !== undefined) {
      f(this.exit);
      return () => {};
    }
    this.observers.push(f);
    return () => {
      const i = this.observers.indexOf(f);
      if (i >= 0) this.observers.splice(i, 1);
    };
  }

  /** The cause an interruption delivers. */
  stopped(): Cause {
    return interrupt(this.requested);
  }

  /** Whether it was asked to stop and may be stopped now. */
  interrupted(): boolean {
    return this.requested !== 0 && this.mask === 0;
  }

  /** Moves a suspended fiber on: at once on this stack, as a wait that ended does. */
  wake(mode: 0 | 1, value: unknown): void {
    this.suspended = false;
    this.cancel = undefined;
    this.mode = mode;
    this.value = value;
    if (this.nested === 0) evaluate(this);
  }

  /** Asks it to stop; a suspended interruptible fiber stops waiting at once. */
  interrupt(by: number): void {
    if (this.exit !== undefined) return;
    if (this.requested === 0) this.requested = by;
    if (this.suspended && this.mask === 0) {
      this.cancel?.();
      this.wake(THROW, new Raise(this.stopped()));
    }
  }

  /** Runs frames until the one on top is `base`, or it suspends: whether it suspended. */
  drive(base: Frame | null): boolean {
    for (;;) {
      const frame = this.top;
      if (frame === base) return false;
      if (frame === null) throw new Error("a fiber ran past its base frame");
      if (this.requested !== 0 && this.mask === 0 && this.mode === NEXT) {
        this.mode = THROW;
        this.value = new Raise(this.stopped());
      }
      if (++this.ops >= BUDGET && this.nested === 0) {
        this.ops = 0;
        this.pause();
        return true;
      }
      context = frame.ctx;
      let step: IteratorResult<unknown, unknown>;
      try {
        step = this.mode === THROW ? frame.gen.throw(this.value) : frame.gen.next(this.value);
      } catch (thrown) {
        this.top = frame.prev;
        this.mode = THROW;
        this.value = thrown;
        continue;
      }
      const value = step.value;
      if (step.done === true) {
        if (isRun(value)) {
          frame.gen = value;
          this.mode = NEXT;
          this.value = undefined;
        } else {
          this.top = frame.prev;
          this.mode = NEXT;
          this.value = value;
          if (value instanceof Instruction && this.instruct(value)) return true;
        }
      } else if (isRun(value)) {
        this.top = new Frame(value, frame.ctx, frame, null, false);
        this.mode = NEXT;
        this.value = undefined;
      } else if (value instanceof Instruction) {
        this.mode = NEXT;
        this.value = undefined;
        if (this.instruct(value)) return true;
      } else {
        this.mode = NEXT;
        this.value = value;
      }
    }
  }

  /** Gives others a turn, keeping what it is about to do. */
  private pause(): void {
    let waiting = true;
    this.suspended = true;
    this.cancel = (): void => {
      waiting = false;
    };
    schedule(() => {
      if (waiting) this.wake(this.mode, this.value);
    });
  }

  private instruct(instruction: Instruction): boolean {
    try {
      return instruction.exec(this);
    } catch (thrown) {
      this.mode = THROW;
      this.value = thrown;
      return false;
    }
  }

  end(): void {
    this.exit = this.mode === THROW ? failed(causeOf(this.value)) : succeeded(this.value);
    const observers = this.observers;
    this.observers = [];
    for (const f of observers) f(this.exit);
  }
}

/** Runs `fiber` on this stack until it suspends or ends. */
const evaluate = (fiber: Fiber): void => {
  const [fiberBefore, contextBefore] = [current, context];
  current = fiber;
  try {
    if (!fiber.drive(null)) fiber.end();
  } finally {
    current = fiberBefore;
    context = contextBefore;
  }
};

// The scheduler: tasks run in order on microtasks, a timer now and then so that
// timers and I/O get their turn.
let tasks: Array<() => void> = [];
let running = false;
const TICKS_BEFORE_TIMER = 2048;

const round = (depth: number): void => {
  const batch = tasks;
  tasks = [];
  for (const task of batch) task();
  if (tasks.length === 0) running = false;
  else starve(depth);
};

const starve = (depth: number): void => {
  if (depth >= TICKS_BEFORE_TIMER) {
    setTimeout(() => round(0), 0);
  } else {
    queueMicrotask(() => round(depth + 1));
  }
};

/** Runs `task` on a later turn of the scheduler. */
export const schedule = (task: () => void): void => {
  tasks.push(task);
  if (!running) {
    running = true;
    starve(0);
  }
};

/** Runs tasks until `done` holds; whether it does. A Sync stage waits for the fibers it needs this way. */
const runTasksUntil = (done: () => boolean): boolean => {
  while (!done()) {
    const task = tasks.shift();
    if (task === undefined) return false;
    task();
  }
  return true;
};

/** Starts a fiber of `task`, with the evidence `ctx`, on a later turn. */
export const fork = (task: () => unknown, ctx: Ctx, mask = 0): Fiber => {
  const fiber = new Fiber();
  fiber.mask = mask;
  fiber.top = new Frame(started(task), ctx, null, null, false);
  fiber.suspended = true;
  schedule(() => {
    if (fiber.suspended && !fiber.started && fiber.exit === undefined) {
      fiber.started = true;
      fiber.wake(NEXT, undefined);
    }
  });
  return fiber;
};

// oxlint-disable-next-line require-yield -- what it returns is a call, or a value
function* started(task: () => unknown): Run {
  return task();
}

/** Starts a fiber of `task` on this stack, up to where it first suspends. */
export const forkNow = (task: () => unknown, ctx: Ctx, mask = 0): Fiber => {
  const fiber = new Fiber();
  fiber.mask = mask;
  fiber.top = new Frame(started(task), ctx, null, null, false);
  fiber.started = true;
  evaluate(fiber);
  return fiber;
};

// oxlint-disable-next-line require-yield -- the driver runs what it returns
function* instructed(instruction: Instruction): Run {
  return instruction;
}

/**
 * Runs what a runtime call gave to its end, in the fiber at hand, on this
 * stack: what a Sync stage does with a function of a row it knows holds nothing
 * that suspends. A wait for another fiber of such a row runs that fiber here.
 */
export const runIn = (run: unknown): unknown => {
  const gen = isRun(run) ? run : run instanceof Instruction ? instructed(run) : undefined;
  if (gen === undefined) return run;
  const outside = current === undefined;
  const fiber = currentFiber();
  const saved = context;
  const base = fiber.top;
  fiber.top = new Frame(gen, context, base, null, true);
  fiber.mode = NEXT;
  fiber.value = undefined;
  fiber.nested++;
  try {
    while (fiber.drive(base)) {
      if (!runTasksUntil(() => !fiber.suspended)) {
        fiber.suspended = false;
        fiber.cancel?.();
        fiber.cancel = undefined;
        fiber.mode = THROW;
        fiber.value = new Error("a stage that runs at once waited for something that has not happened");
      }
    }
  } finally {
    fiber.nested--;
    context = saved;
    if (outside) current = undefined;
  }
  if (fiber.mode === THROW) throw fiber.value;
  return fiber.value;
};

/** A wait: `start` gets the functions that end it, and may give what stops it. */
export class Suspend extends Instruction {
  private readonly start: (resolve: (value: unknown) => void, reject: (error: unknown) => void) => (() => void) | void;
  constructor(start: (resolve: (value: unknown) => void, reject: (error: unknown) => void) => (() => void) | void) {
    super();
    this.start = start;
  }
  exec(fiber: Fiber): boolean {
    // still starting: an end now moves the fiber on without a wake
    let starting = true;
    let ended = false;
    const end = (mode: 0 | 1, value: unknown): void => {
      if (ended) return;
      ended = true;
      if (!starting) {
        fiber.wake(mode, value);
        return;
      }
      fiber.mode = mode;
      fiber.value = value;
    };
    fiber.suspended = true;
    fiber.cancel = undefined;
    const cancel = this.start(
      (v) => end(NEXT, v),
      (e) => end(THROW, e),
    );
    starting = false;
    if (ended) {
      fiber.suspended = false;
      if (cancel !== undefined) cancel();
      return false;
    }
    fiber.cancel = (): void => {
      ended = true;
      if (cancel !== undefined) cancel();
    };
    return true;
  }
}

/** `thunk` run with the evidence `ctx`, as a frame of its own if it is a call. */
export class Under extends Instruction {
  private readonly ctx: Ctx;
  private readonly thunk: () => unknown;
  private readonly prompt: object | null;
  constructor(ctx: Ctx, thunk: () => unknown, prompt: object | null = null) {
    super();
    this.ctx = ctx;
    this.thunk = thunk;
    this.prompt = prompt;
  }
  exec(fiber: Fiber): boolean {
    const value = inCtx(this.ctx, this.thunk);
    if (isRun(value)) {
      fiber.top = new Frame(value, this.ctx, fiber.top, this.prompt, false);
    } else if (value instanceof Instruction) {
      fiber.top = new Frame(instructed(value), this.ctx, fiber.top, this.prompt, false);
    } else {
      fiber.value = value;
    }
    return false;
  }
}

/** Gives the fiber's turn to the others. */
export const yieldNow = (): Instruction =>
  new Suspend((resolve) => {
    schedule(() => resolve(undefined));
  });

/** Waits for `fiber` to end; its exit. */
export const awaitFiber = (fiber: Fiber): Instruction =>
  new Suspend((resolve) => fiber.observe(resolve));

/** `task` at the mask depth `mask`, which ends with it. */
export function* atMask(mask: number, task: () => unknown): Run {
  const fiber = currentFiber();
  const saved = fiber.mask;
  fiber.mask = mask;
  try {
    return yield task();
  } finally {
    fiber.mask = saved;
  }
}

/** `task` one mask deeper. */
export function* uninterruptible(task: () => unknown): Run {
  const fiber = currentFiber();
  fiber.mask++;
  try {
    return yield task();
  } finally {
    fiber.mask--;
  }
}

/** How `task` ends, as a value; an interruption delivered to it is an ending like any other. */
export function* exitOf(task: () => unknown): Run {
  try {
    return succeeded(yield task());
  } catch (thrown) {
    return failed(causeOf(thrown));
  }
}

/**
 * A failure in a fiber asked to stop while it is interruptible skips the
 * handler that caught it: its typed failures dropped, the interruption added.
 * Each recovery calls it; what must run whatever happens does not.
 */
export const skipHandler = (fiber: Fiber, cause: Cause): void => {
  if (!fiber.interrupted()) return;
  const kept = fromReasons(cause.reasons.filter((r) => !isFail(r)));
  const stop = fiber.stopped();
  const [reason] = stop.reasons;
  const has = reason !== undefined && kept.reasons.some((r) => sameReason(r, reason));
  return raise(has ? kept : concat(kept, stop));
};

/** `task` under a recovery: how it ended, unless an interruption skips it. */
export function* caught(task: () => unknown): Run {
  try {
    return succeeded(yield task());
  } catch (thrown) {
    const cause = causeOf(thrown);
    skipHandler(currentFiber(), cause);
    return failed(cause);
  }
}

/** What a runtime step gave, known to be an exit. */
export const asExit = (value: unknown): Exit => {
  if (value instanceof Exit) return value;
  throw new TypeError("not an exit");
};

/** The value of `exit`, or its failure thrown. */
export const raiseOf = (exit: Exit): unknown => {
  if (exit.cause !== undefined) return raise(exit.cause);
  return exit.value;
};
