// Event handlers and their policies. An event is snapshotted and its
// default action decided synchronously, inside the DOM callback; its task
// then runs as a fiber of the root's bridge under the handler's policy.
import { Cause, type Context, Effect, Exit, Fiber } from "effect";

import { Failure } from "../../cause.ts";
import type { Binding, DomEvent } from "./nodes.ts";

export type Policy =
  | { readonly kind: "exhaust" }
  | { readonly kind: "latest" }
  | { readonly kind: "queue"; readonly capacity: number }
  | { readonly kind: "concurrent"; readonly limit: number };

/** `Ui.Handler s r e`. */
export interface Handler {
  readonly policy: Policy;
  readonly run: (snapshot: unknown) => Effect.Effect<unknown, unknown>;
  /** A pure decision, taken while the browser waits, to prevent the default action. */
  readonly prevent?: (snapshot: unknown) => boolean;
}

const makeHandler = (policy: Policy, run: Handler["run"]): Handler => Object.freeze({ policy, run });

const positive = (what: string, n: number): number => {
  if (!Number.isInteger(n) || n <= 0) {
    throw new RangeError(`the ${what} of an event policy must be a positive integer, not ${String(n)}`);
  }
  return n;
};

export const eventExhaust = (run: Handler["run"]): Handler => makeHandler({ kind: "exhaust" }, run);
export const eventLatest = (run: Handler["run"]): Handler => makeHandler({ kind: "latest" }, run);
export const eventQueue = (capacity: number, run: Handler["run"]): Handler =>
  makeHandler({ kind: "queue", capacity: positive("capacity", capacity) }, run);
export const eventConcurrent = (limit: number, run: Handler["run"]): Handler =>
  makeHandler({ kind: "concurrent", limit: positive("limit", limit) }, run);
export const eventPreventDefault = (decide: (snapshot: unknown) => boolean, inner: Handler): Handler =>
  Object.freeze({ ...inner, prevent: decide });

type Task = Fiber.Fiber<unknown, unknown>;

/** The policy state of one event prop at one place of the tree, kept across renders. */
interface Slot {
  readonly running: Set<Task>;
  /** `queue`: the events waiting; `latest`: the one event waiting. */
  readonly waiting: Array<{ readonly handler: Handler; readonly snapshot: unknown }>;
}

/**
 * The runtime side of a mounted root: which events it still takes, the
 * tasks it runs and the supervisor that hears of their failures.
 */
export class Bridge<R> {
  accepting = true;
  readonly tasks = new Set<Task>();
  readonly supervisors = new Set<Task>();
  private readonly slots = new Map<string, Slot>();
  private readonly context: Context.Context<R>;
  private readonly supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>;
  private readonly settled: () => void;
  private readonly broken: (cause: Cause.Cause<unknown>) => void;

  /**
   * `settled` runs after each task ends while the root is open: the root
   * renders again. `broken` gets the failure of the supervisor itself.
   */
  constructor(
    context: Context.Context<R>,
    supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R>,
    settled: () => void,
    broken: (cause: Cause.Cause<unknown>) => void,
  ) {
    this.context = context;
    this.supervisor = supervisor;
    this.settled = settled;
    this.broken = broken;
  }

  /** The DOM callback of `binding` at `place`. */
  listener(place: string, binding: Binding): (event: DomEvent) => void {
    return (event) => this.dispatch(place, binding, event);
  }

  private dispatch(place: string, { handler, snapshot: take }: Binding, event: DomEvent): void {
    if (!this.accepting) return;
    const snapshot = take(event);
    if (handler.prevent?.(snapshot) === true) event.preventDefault();
    const slot = this.slots.get(place) ?? { running: new Set<Task>(), waiting: [] };
    this.slots.set(place, slot);
    const { policy } = handler;
    switch (policy.kind) {
      case "exhaust":
        if (slot.running.size === 0) this.start(slot, handler, snapshot);
        return;
      case "latest":
        slot.waiting.splice(0, slot.waiting.length, { handler, snapshot });
        if (slot.running.size === 0) this.next(slot);
        else for (const task of slot.running) Effect.runFork(Fiber.interrupt(task));
        return;
      case "queue":
        if (slot.running.size === 0) this.start(slot, handler, snapshot);
        else if (slot.waiting.length < policy.capacity) slot.waiting.push({ handler, snapshot });
        else this.overflow(`the queue of a handler is full: ${policy.capacity} events are waiting`);
        return;
      case "concurrent":
        if (slot.running.size < policy.limit) this.start(slot, handler, snapshot);
        else this.overflow(`a handler already runs ${policy.limit} tasks`);
        return;
    }
  }

  private next(slot: Slot): void {
    const waiting = slot.waiting.shift();
    if (waiting !== undefined) this.start(slot, waiting.handler, waiting.snapshot);
  }

  private start(slot: Slot, handler: Handler, snapshot: unknown): void {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker gave the handlers of a root the services of its `Ui.run`
    const task = Effect.suspend(() => handler.run(snapshot) as Effect.Effect<unknown, unknown, R>);
    const fiber = Effect.runFork(Effect.provideContext(task, this.context));
    slot.running.add(fiber);
    this.tasks.add(fiber);
    fiber.addObserver((exit) => {
      slot.running.delete(fiber);
      this.tasks.delete(fiber);
      // an interruption is the policy's or the root's own doing
      if (Exit.isFailure(exit) && !Cause.hasInterruptsOnly(exit.cause)) this.supervise(exit.cause);
      if (!this.accepting) return;
      this.settled();
      if (slot.running.size === 0) this.next(slot);
    });
  }

  /** An event a full policy rejects: an infrastructure defect, not an error of `e`. */
  private overflow(message: string): void {
    this.supervise(Cause.die(new Error(`event rejected: ${message}`)));
  }

  private supervise(cause: Cause.Cause<unknown>): void {
    const run = Effect.exit(Effect.suspend(() => this.supervisor(new Failure(cause))));
    const fiber = Effect.runFork(Effect.provideContext(run, this.context));
    this.supervisors.add(fiber);
    fiber.addObserver((exit) => {
      this.supervisors.delete(fiber);
      if (Exit.isSuccess(exit) && Exit.isFailure(exit.value)) this.broken(exit.value.cause);
    });
  }
}
