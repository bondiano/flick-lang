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
  /** An `Action`: every element that uses it shares one policy state per root. */
  readonly shared?: boolean;
  /** `Action.with base arg`: the action whose policy state it shares. */
  readonly base?: Handler;
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

const makeAction = (policy: Policy, run: Handler["run"]): Handler => Object.freeze({ policy, run, shared: true });

export const actionExhaust = (run: Handler["run"]): Handler => makeAction({ kind: "exhaust" }, run);
export const actionLatest = (run: Handler["run"]): Handler => makeAction({ kind: "latest" }, run);
export const actionQueue = (capacity: number, run: Handler["run"]): Handler =>
  makeAction({ kind: "queue", capacity: positive("capacity", capacity) }, run);
export const actionConcurrent = (limit: number, run: Handler["run"]): Handler =>
  makeAction({ kind: "concurrent", limit: positive("limit", limit) }, run);

/** `Action.with action arg`: the action given `arg` with each snapshot, sharing its policy state. */
export const actionWith = (action: Handler, arg: unknown): Handler => {
  const { prevent } = action;
  return Object.freeze({
    policy: action.policy,
    run: (snapshot: unknown) => action.run([arg, snapshot]),
    shared: true,
    base: action.base ?? action,
    ...(prevent === undefined ? {} : { prevent: (snapshot: unknown) => prevent([arg, snapshot]) }),
  });
};

type Task = Fiber.Fiber<unknown, unknown>;

/** The policy state of one mounted event prop, kept across renders. */
interface Slot {
  binding: Binding | undefined;
  readonly running: Set<Task>;
  /** `queue`: the events waiting; `latest`: the one event waiting. */
  readonly waiting: Array<{ readonly handler: Handler; readonly snapshot: unknown }>;
}

export interface Listener {
  readonly callback: (event: DomEvent) => void;
  update(binding: Binding): void;
  close(): void;
}

/**
 * The runtime side of a mounted root: which events it still takes, the
 * tasks it runs and the supervisor that hears of their failures.
 */
export class Bridge<R> {
  accepting = true;
  readonly tasks = new Set<Task>();
  readonly supervisors = new Set<Task>();
  /** The policy state of each `Action`, shared by the elements that use it. */
  private readonly actions = new Map<Handler, Slot>();
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

  /** One committed event prop owns its policy and reuses its callback across renders. */
  listener(binding: Binding): Listener {
    if (binding.handler.shared === true) return this.actionListener(binding);
    const slot: Slot = { binding, running: new Set<Task>(), waiting: [] };
    return {
      callback: (event) => {
        if (slot.binding !== undefined) this.dispatch(slot, slot.binding, event);
      },
      update: (current) => {
        slot.binding = current;
      },
      close: () => {
        slot.binding = undefined;
        slot.waiting.length = 0;
      },
    };
  }

  /**
   * An event prop of an `Action`: the action's state outlives the element,
   * since other elements may use it, and its tasks belong to the root.
   */
  private actionListener(initial: Binding): Listener {
    let current = initial;
    const key = initial.handler.base ?? initial.handler;
    let slot = this.actions.get(key);
    if (slot === undefined) {
      slot = { binding: initial, running: new Set<Task>(), waiting: [] };
      this.actions.set(key, slot);
    }
    const shared = slot;
    return {
      callback: (event) => this.dispatch(shared, current, event),
      update: (binding) => {
        current = binding;
      },
      close: () => undefined,
    };
  }

  private dispatch(slot: Slot, { handler, snapshot: take }: Binding, event: DomEvent): void {
    if (!this.accepting) return;
    const snapshot = take(event);
    if (handler.prevent?.(snapshot) === true) event.preventDefault();
    const { policy } = handler;
    switch (policy.kind) {
      case "exhaust":
        if (slot.running.size === 0) this.start(slot, handler, snapshot);
        return;
      case "latest":
        slot.waiting.splice(0, slot.waiting.length, { handler, snapshot });
        if (slot.running.size === 0) this.next(slot);
        else {
          // Interrupting an old task can synchronously start its replacement.
          const running = [...slot.running];
          for (const task of running) Effect.runFork(Fiber.interrupt(task));
        }
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
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker gave the handlers the services of their root
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
      if (slot.binding !== undefined && slot.running.size === 0) this.next(slot);
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
