// Event handlers and their policies. An event is snapshotted and its
// default action decided synchronously, inside the DOM callback; its task
// then runs as a fiber of the root's bridge under the handler's policy.
import { Cause, Context, Effect, Exit, Fiber, type Tracer } from "effect";

import { Failure } from "../../cause.ts";
import type { Binding, DomEvent } from "@flick-lang/runtime/ui/react/nodes";
import type { Handler, Listener } from "@flick-lang/runtime/ui/react/handlers";
import { ActionObserver } from "../action-observer.ts";

export * from "@flick-lang/runtime/ui/react/handlers";

type Task = Fiber.Fiber<unknown, unknown>;

/** An event the policy of `slot` turned away, an event of the span of a task it runs. */
const rejected = (slot: Slot, event: string, reason: string): void => {
  for (const span of slot.spans) span.event("rejected", now(), { "flick.event": event, reason });
};

/** The policy state of one mounted event prop, kept across renders. */
interface Slot {
  binding: Binding | undefined;
  readonly running: Set<Task>;
  /** The spans of the running tasks, which hear of the events the policy rejects. */
  readonly spans: Set<Tracer.Span>;
  /** `queue`: the events waiting; `latest`: the one event waiting. */
  readonly waiting: Array<{ readonly handler: Handler; readonly snapshot: unknown; readonly event: string }>;
}

const now = (): bigint => BigInt(Date.now()) * 1_000_000n;

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
  private readonly observer: ActionObserver | undefined;

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
    this.observer = Context.get(context, ActionObserver);
  }

  /**
   * One committed event prop owns its policy and reuses its callback across
   * renders; the tasks it started belong to the root, so a save outlives
   * the dialog whose button started it.
   */
  listener(binding: Binding): Listener {
    if (binding.handler.shared === true) return this.actionListener(binding);
    const slot: Slot = { binding, running: new Set<Task>(), spans: new Set<Tracer.Span>(), waiting: [] };
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
    const key = initial.handler;
    let slot = this.actions.get(key);
    if (slot === undefined) {
      slot = { binding: initial, running: new Set<Task>(), spans: new Set<Tracer.Span>(), waiting: [] };
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

  /**
   * The instance that made `actions` unmounts: their events stop and their
   * tasks are interrupted. A task stays among the root's until it ends, so
   * the root's teardown still waits for it.
   */
  release(actions: ReadonlyArray<Handler>): void {
    for (const action of actions) {
      const slot = this.actions.get(action);
      if (slot === undefined) continue;
      this.actions.delete(action);
      slot.binding = undefined;
      slot.waiting.length = 0;
      for (const task of slot.running) Effect.runFork(Fiber.interrupt(task));
    }
  }

  private dispatch(slot: Slot, { handler, snapshot: take }: Binding, event: DomEvent): void {
    if (!this.accepting) return;
    const snapshot = take(event);
    if (handler.prevent?.(snapshot) === true) event.preventDefault();
    const { policy } = handler;
    const { type } = event;
    switch (policy.kind) {
      case "exhaust":
        if (slot.running.size === 0) this.start(slot, handler, snapshot, type);
        else rejected(slot, type, "a task of the handler runs");
        return;
      case "latest":
        if (slot.waiting.length > 0) rejected(slot, slot.waiting[0]?.event ?? type, "a later event replaced it");
        slot.waiting.splice(0, slot.waiting.length, { handler, snapshot, event: type });
        if (slot.running.size === 0) this.next(slot);
        else {
          // Interrupting an old task can synchronously start its replacement.
          const running = [...slot.running];
          for (const task of running) Effect.runFork(Fiber.interrupt(task));
        }
        return;
      case "queue":
        if (slot.running.size === 0) this.start(slot, handler, snapshot, type);
        else if (slot.waiting.length < policy.capacity) slot.waiting.push({ handler, snapshot, event: type });
        else {
          rejected(slot, type, "the queue is full");
          this.overflow(`the queue of a handler is full: ${policy.capacity} events are waiting`);
        }
        return;
      case "concurrent":
        if (slot.running.size < policy.limit) this.start(slot, handler, snapshot, type);
        else {
          rejected(slot, type, "the limit of tasks runs");
          this.overflow(`a handler already runs ${policy.limit} tasks`);
        }
        return;
    }
  }

  private next(slot: Slot): void {
    const waiting = slot.waiting.shift();
    if (waiting !== undefined) this.start(slot, waiting.handler, waiting.snapshot, waiting.event);
  }

  /** Runs an event; spans are opt-in for this root. */
  private start(slot: Slot, handler: Handler, snapshot: unknown, event: string): void {
    const task = (): Effect.Effect<unknown, unknown> => {
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- on Effect, the task of an event is an Effect
      return handler.run(snapshot) as Effect.Effect<unknown, unknown>;
    };
    const traced = this.observer?.(handler, event, slot.spans, task) ?? Effect.suspend(task);
    this.fork(
      traced,
      (fiber) => slot.running.add(fiber),
      (fiber) => {
        slot.running.delete(fiber);
        if (!this.accepting) return;
        this.settled();
        if (slot.binding !== undefined && slot.running.size === 0) this.next(slot);
      },
    );
  }

  /**
   * Runs `task` as a task of the root, with its services; a failure other
   * than an interruption goes to the supervisor. `started` sees the fiber
   * before it can end, `ended` after.
   */
  fork(task: Effect.Effect<unknown, unknown>, started?: (fiber: Task) => void, ended?: (fiber: Task) => void): Task {
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the checker gave the tasks the services of their root
    const fiber = Effect.runFork(Effect.provideContext(task as Effect.Effect<unknown, unknown, R>, this.context));
    started?.(fiber);
    this.tasks.add(fiber);
    fiber.addObserver((exit) => {
      this.tasks.delete(fiber);
      // an interruption is the policy's or the root's own doing
      if (Exit.isFailure(exit) && !Cause.hasInterruptsOnly(exit.cause)) this.supervise(exit.cause);
      ended?.(fiber);
    });
    return fiber;
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
