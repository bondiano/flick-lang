// The bindings of `Queue` on the kernel, as Go's `flickrt` has Effect 4's:
// messages up to a capacity, a strategy for a full queue, and an end. Failing
// or ending a queue lets takers drain what is left, then fail as it was failed;
// the end reaches Flick as `None`. A wait that ends is moved on from the
// scheduler, as Effect releases takers from its dispatcher, so a queue is never
// changed under a loop over its waiters.
import { type Option, none, some } from "../core/data.ts";
import { tag } from "../tags.ts";
import { type Cause, fail, fromReasons, interrupt, isFail, isInterrupt, raise, unwinds } from "../cause.ts";
import { type Run, Suspend, asExit, atMask, currentFiber, exitOf, schedule } from "../fiber.ts";
import type { Failure } from "../outcome.ts";

/** The error of the cause that ends a queue: `Cause.done` of Effect. */
const END: object = /*#__PURE__*/ Object.freeze({});
const ENDED: Cause = /*#__PURE__*/ fail(END);

const isEnd = (reason: Cause["reasons"][number]): boolean => isFail(reason) && reason.error === END;

/**
 * What a cause that ended a queue leaves: nothing when it was only the end, or
 * the cause without it when something besides interruptions failed too.
 */
const filterDone = (cause: Cause): Cause | undefined => {
  if (!cause.reasons.some(isEnd)) return cause;
  if (cause.reasons.every((r) => isEnd(r) || isInterrupt(r))) return undefined;
  return fromReasons(cause.reasons.filter((r) => !isEnd(r)));
};

type Strategy = "suspend" | "dropping" | "sliding";

const OPEN = 0;
const CLOSING = 1;
const DONE = 2;

/** A take that waits until `ready`. */
interface Taker {
  readonly ready: () => boolean;
  readonly wake: () => void;
}

/** An offer that waits for room: one message, or `remaining` from `offset` on. */
interface Offer {
  readonly single: boolean;
  readonly message: unknown;
  readonly remaining: ReadonlyArray<unknown>;
  offset: number;
  accepted: boolean;
  left: ReadonlyArray<unknown>;
  wake: () => void;
}

const ignore = (): void => {};

/** A wait for `resume` to be called, which moves the fiber on from the scheduler. */
const waiting = (register: (resume: () => void) => void): Suspend =>
  new Suspend((resolve) => {
    register(() => schedule(() => resolve(undefined)));
  });

export class Queue {
  readonly capacity: number;
  readonly strategy: Strategy;
  messages: Array<unknown> = [];
  state = OPEN;
  /** What takes fail with once it closes. */
  exit: Cause = ENDED;
  takers: Array<Taker> = [];
  offers: Array<Offer> = [];
  readonly ended: Array<() => void> = [];

  constructor(capacity: number, strategy: Strategy) {
    this.capacity = capacity;
    this.strategy = strategy;
  }

  offerNow(message: unknown): boolean {
    if (this.state !== OPEN) return false;
    if (this.messages.length >= this.capacity) {
      if (this.strategy === "sliding") {
        if (this.messages.length > 0) this.messages.shift();
        this.messages.push(message);
        return true;
      }
      if (this.capacity <= 0 && this.takers.length > 0) {
        this.messages.push(message);
        this.releaseTakers();
        return true;
      }
      return false;
    }
    this.messages.push(message);
    this.releaseTakers();
    return true;
  }

  /** Adds what fits; the messages not added. */
  offerAllNow(messages: ReadonlyArray<unknown>): Array<unknown> {
    if (this.state !== OPEN) return [...messages];
    if (this.capacity === Infinity || this.strategy === "sliding") {
      this.messages.push(...messages);
      const over = this.messages.length - this.capacity;
      if (this.strategy === "sliding" && over > 0) this.messages.splice(0, over);
      this.releaseTakers();
      return [];
    }
    const free = this.capacity <= 0 ? this.takers.length : this.capacity - this.messages.length;
    if (free <= 0) return [...messages];
    this.messages.push(...messages.slice(0, free));
    this.releaseTakers();
    return messages.slice(free);
  }

  /** Ends it with `cause` once it is drained; whether this call ended it. */
  failCause(cause: Cause): boolean {
    if (this.state !== OPEN) return false;
    if (this.offers.length === 0 && this.messages.length === 0) {
      this.finalize(cause);
      return true;
    }
    this.state = CLOSING;
    this.exit = cause;
    this.releaseTakers();
    return true;
  }

  /** Ends it at once, dropping its messages and offers. */
  shutdown(): boolean {
    if (this.state === DONE) return false;
    this.messages = [];
    const offers = this.offers;
    this.finalize(this.state === OPEN ? interrupt(undefined) : this.exit);
    for (const o of offers) {
      if (!o.single) o.left = o.remaining.slice(o.offset);
      o.wake();
    }
    return true;
  }

  /** Wakes the takers that are ready, in order, until the messages run out. */
  releaseTakers(): void {
    if (this.state === DONE) return;
    for (let i = 0; i < this.takers.length; ) {
      const t = this.takers[i];
      if (t === undefined || !t.ready()) {
        i++;
        continue;
      }
      this.takers.splice(i, 1);
      t.wake();
      if (this.messages.length === 0) break;
    }
  }

  canTake(lo: number): boolean {
    let need = Math.min(lo, this.capacity === 0 ? 1 : this.capacity);
    if (this.state === CLOSING) need = 1;
    return this.messages.length >= need || (this.capacity <= 0 && this.state !== DONE && this.offers.length > 0);
  }

  /** The first message of the first offer. */
  takeOffer(): unknown {
    const o = this.offers[0];
    if (o === undefined) return undefined;
    if (o.single) {
      this.offers.shift();
      o.accepted = true;
      o.wake();
      return o.message;
    }
    const message = o.remaining[o.offset];
    o.offset++;
    if (o.offset === o.remaining.length) {
      this.offers.shift();
      o.left = [];
      o.wake();
    }
    return message;
  }

  /** Moves waiting offers into the room there is; ends a closing queue once drained. */
  releaseCapacity(): void {
    if (this.state === DONE) return;
    if (this.offers.length === 0) {
      if (this.state === CLOSING && this.messages.length === 0) this.finalize(this.exit);
      return;
    }
    for (let o = this.offers[0]; o !== undefined; o = this.offers[0]) {
      let room = this.capacity - this.messages.length;
      if (room <= 0) return;
      if (o.single) {
        this.messages.push(o.message);
        o.accepted = true;
      } else {
        for (; o.offset < o.remaining.length; o.offset++) {
          if (room === 0) return;
          this.messages.push(o.remaining[o.offset]);
          room--;
        }
        o.left = [];
      }
      this.offers.shift();
      o.wake();
    }
  }

  finalize(exit: Cause): void {
    if (this.state === DONE) return;
    this.state = DONE;
    this.exit = exit;
    const waiters = [...this.takers.map((t) => t.wake), ...this.ended];
    this.takers = [];
    this.offers = [];
    this.ended.length = 0;
    for (const wake of waiters) wake();
  }

  /** A message, or false when a take must wait; throws the cause of a done queue. */
  takeNow(): readonly [unknown] | false {
    if (this.state === DONE) return raise(this.exit);
    let message: unknown;
    if (this.messages.length > 0) message = this.messages.shift();
    else if (this.capacity <= 0 && this.offers.length > 0) message = this.takeOffer();
    else return false;
    this.releaseCapacity();
    return [message];
  }

  takeBetweenNow(lo: number, hi: number): Array<unknown> | false {
    if (this.state === DONE) return raise(this.exit);
    if (hi <= 0 || lo <= 0) return [];
    if (!this.canTake(lo)) return false;
    const taken = this.messages.length > 0 ? this.messages.splice(0, Math.min(hi, this.messages.length)) : [this.takeOffer()];
    this.releaseCapacity();
    return taken;
  }
}

/** Waits until `ready`, or fails as the queue ended. */
function* awaitTake(q: Queue, ready: () => boolean): Run {
  if (q.state === DONE) return raise(q.exit);
  if (ready()) return undefined;
  let taker: Taker | undefined;
  try {
    yield waiting((wake) => {
      taker = { ready, wake };
      q.takers.push(taker);
    });
  } catch (thrown) {
    if (q.state !== DONE && taker !== undefined) {
      const i = q.takers.indexOf(taker);
      // a taker woken but gone passes its wake on
      if (i >= 0) q.takers.splice(i, 1);
      else q.releaseTakers();
    }
    throw thrown;
  }
  return undefined;
}

/** Waits for `o` to settle; an interrupted one is withdrawn. */
function* awaitOffer(q: Queue, o: Offer): Run {
  try {
    yield waiting((wake) => {
      o.wake = wake;
      q.offers.push(o);
    });
  } catch (thrown) {
    if (q.state !== DONE) {
      const i = q.offers.indexOf(o);
      if (i >= 0) q.offers.splice(i, 1);
      if (q.state === CLOSING && q.offers.length === 0 && q.messages.length === 0) q.finalize(q.exit);
    }
    throw thrown;
  }
  return undefined;
}

function* takeOne(q: Queue): Run {
  for (;;) {
    const taken = q.takeNow();
    if (taken !== false) return taken[0];
    yield awaitTake(q, () => q.canTake(1));
  }
}

function* takeBetween(q: Queue, lo: number, hi: number): Run {
  const min = Math.max(lo, 0);
  const max = Math.max(hi, 0);
  for (;;) {
    const taken = q.takeBetweenNow(min, max);
    if (taken !== false) return taken;
    yield awaitTake(q, () => q.canTake(min));
  }
}

function* peekOne(q: Queue): Run {
  for (;;) {
    if (q.state === DONE) return raise(q.exit);
    if (q.messages.length > 0) return q.messages[0];
    yield awaitTake(q, () => q.messages.length > 0);
  }
}

/** `Some` of what `task` gives, `None` once the queue has ended. */
function* orDone(task: () => unknown): Run {
  const exit = asExit(yield exitOf(task));
  if (exit.cause === undefined) return some(exit.value);
  const rest = filterDone(exit.cause);
  return rest === undefined ? none : raise(rest);
}

const strategies = /*#__PURE__*/ new Map<string, Strategy>([
  [tag("Queue.Strategy.Suspend"), "suspend"],
  [tag("Queue.Strategy.Dropping"), "dropping"],
  [tag("Queue.Strategy.Sliding"), "sliding"],
]);

export const flickMake = (options: {
  readonly capacity: Option<number>;
  readonly strategy: Option<{ readonly _tag: string }>;
}): Queue =>
  new Queue(
    options.capacity._tag === "Some" ? options.capacity.value : Infinity,
    options.strategy._tag === "Some" ? (strategies.get(options.strategy.value._tag) ?? "suspend") : "suspend",
  );
export const bounded = (capacity: number): Queue => new Queue(capacity, "suspend");
export const sliding = (capacity: number): Queue => new Queue(capacity, "sliding");
export const dropping = (capacity: number): Queue => new Queue(capacity, "dropping");
export const unbounded = (): Queue => new Queue(Infinity, "suspend");

export const offer =
  (message: unknown) =>
  (q: Queue): Run =>
    (function* (): Run {
      if (q.offerNow(message)) return true;
      if (q.state !== OPEN || q.strategy !== "suspend") return false;
      const o: Offer = { single: true, message, remaining: [], offset: 0, accepted: false, left: [], wake: ignore };
      yield awaitOffer(q, o);
      return o.accepted;
    })();

export const offerAll =
  (messages: ReadonlyArray<unknown>) =>
  (q: Queue): Run =>
    (function* (): Run {
      const remaining = q.offerAllNow(messages);
      if (remaining.length === 0 || q.strategy === "dropping" || q.state !== OPEN) return remaining;
      const o: Offer = { single: false, message: undefined, remaining, offset: 0, accepted: false, left: [], wake: ignore };
      yield awaitOffer(q, o);
      return o.left;
    })();

const failWith =
  (error: unknown) =>
  (q: Queue): boolean =>
    q.failCause(fail(error));
export { failWith as fail };
export const flickFailCause =
  (failure: Failure) =>
  (q: Queue): boolean =>
    q.failCause(failure.cause);
export const end = (q: Queue): boolean => q.failCause(ENDED);
const interruptIt = (q: Queue): boolean => q.failCause(interrupt(currentFiber().id));
export { interruptIt as interrupt };
export const shutdown = (q: Queue): boolean => q.shutdown();

/** Every message there is, without waiting; a failed queue fails. */
export const clear = (q: Queue): Array<unknown> => {
  if (q.state === DONE) {
    const rest = filterDone(q.exit);
    return rest === undefined ? [] : raise(rest);
  }
  const taken = q.messages.length > 0 ? q.messages.splice(0) : q.offers.length > 0 ? [q.takeOffer()] : [];
  q.releaseCapacity();
  return taken;
};

export const flickTakeAll = (q: Queue): Run => orDone(() => takeBetween(q, 1, Infinity));

const listOf = (value: unknown): ReadonlyArray<unknown> => (Array.isArray(value) ? value : []);

/** Takes messages until the queue ends. */
export function* collect(q: Queue): Run {
  const out: Array<unknown> = [];
  for (;;) {
    const taken = asExit(yield exitOf(() => takeBetween(q, 1, Infinity)));
    if (taken.cause !== undefined) {
      const rest = filterDone(taken.cause);
      return rest === undefined ? out : raise(rest);
    }
    out.push(...listOf(taken.value));
  }
}

export const flickTakeN =
  (n: number) =>
  (q: Queue): Run =>
    orDone(() => takeBetween(q, n, n));
export const flickTakeBetween =
  (lo: number, hi: number) =>
  (q: Queue): Run =>
    orDone(() => takeBetween(q, lo, hi));
export const flickTake = (q: Queue): Run => orDone(() => takeOne(q));
export const flickPeek = (q: Queue): Run => orDone(() => peekOne(q));

export const poll = (q: Queue): Option<unknown> => {
  if (q.state === DONE) return none;
  const taken = q.takeNow();
  return taken === false ? none : some(taken[0]);
};

/** Wakes the takers that are ready now. */
export const flush = (q: Queue): undefined => {
  q.releaseTakers();
  return undefined;
};

/** Waits until the queue has ended, failing as it failed. */
function* awaited(q: Queue): Run {
  if (q.state !== DONE) yield waiting((wake) => q.ended.push(wake));
  const rest = filterDone(q.exit);
  return rest === undefined ? undefined : raise(rest);
}
export { awaited as await };

export const size = (q: Queue): number => (q.state === DONE ? 0 : q.messages.length);
export const isFull = (q: Queue): boolean => size(q) === q.capacity;

/** Runs `task`, then ends the queue, or fails it as the task failed: whether this ended it. */
export const into =
  (q: Queue) =>
  (task: () => unknown): Run =>
    (function* (): Run {
      const fiber = currentFiber();
      const depth = fiber.mask;
      fiber.mask = depth + 1;
      try {
        const exit = asExit(yield exitOf(() => atMask(depth, task)));
        if (exit.cause === undefined) return q.failCause(ENDED);
        if (unwinds(exit.cause)) return raise(exit.cause);
        return q.failCause(exit.cause);
      } finally {
        fiber.mask = depth;
      }
    })();
