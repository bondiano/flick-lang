// The runtime side of a mounted root on Flick's own runtime: an event is
// snapshotted and its default action decided synchronously, inside the DOM
// callback; its task then runs as a fiber of the root under the handler's
// policy, with the root's evidence.
import type { Binding, DomEvent } from "./react/nodes.ts";
import type { Handler, Listener } from "./react/handlers.ts";
import { type Cause, die, hasInterruptsOnly } from "../cause.ts";
import { type Ctx, type Fiber, asExit, exitOf, fork } from "../fiber.ts";
import { Failure } from "../outcome.ts";

/** The policy state of one mounted event prop, kept across renders. */
interface Slot {
  binding: Binding | undefined;
  readonly running: Set<Fiber>;
  /** `queue`: the events waiting; `latest`: the one event waiting. */
  readonly waiting: Array<{ readonly handler: Handler; readonly snapshot: unknown }>;
}

/** A task of the program: a function of nothing that runs when called. */
type Task = () => unknown;

const stop = (fiber: Fiber): void => fiber.interrupt(-1);

export class Bridge {
  accepting = true;
  readonly tasks = new Set<Fiber>();
  readonly supervisors = new Set<Fiber>();
  /** The policy state of each `Action`, shared by the elements that use it. */
  private readonly actions = new Map<Handler, Slot>();
  private readonly ctx: Ctx;
  private readonly supervisor: (failure: Failure) => unknown;
  private readonly broken: (cause: Cause) => void;

  /** `broken` gets the failure of the supervisor itself. */
  constructor(ctx: Ctx, supervisor: (failure: Failure) => unknown, broken: (cause: Cause) => void) {
    this.ctx = ctx;
    this.supervisor = supervisor;
    this.broken = broken;
  }

  /**
   * One committed event prop owns its policy and reuses its callback across
   * renders; the tasks it started belong to the root, so a save outlives
   * the dialog whose button started it.
   */
  listener(binding: Binding): Listener {
    if (binding.handler.shared === true) return this.actionListener(binding);
    const slot: Slot = { binding, running: new Set(), waiting: [] };
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

  /** An event prop of an `Action`: the action's state outlives the element, since other elements may use it. */
  private actionListener(initial: Binding): Listener {
    let current = initial;
    let slot = this.actions.get(initial.handler);
    if (slot === undefined) {
      slot = { binding: initial, running: new Set(), waiting: [] };
      this.actions.set(initial.handler, slot);
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

  /** The instance that made `actions` unmounts: their events stop and their tasks are interrupted. */
  release(actions: ReadonlyArray<Handler>): void {
    for (const action of actions) {
      const slot = this.actions.get(action);
      if (slot === undefined) continue;
      this.actions.delete(action);
      slot.binding = undefined;
      slot.waiting.length = 0;
      slot.running.forEach(stop);
    }
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
        // interrupting an old task can start its replacement at once
        else [...slot.running].forEach(stop);
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
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- on this runtime the task of an event is a function of nothing
    const task = handler.run(snapshot) as Task;
    this.fork(
      task,
      (fiber) => slot.running.add(fiber),
      (fiber) => {
        slot.running.delete(fiber);
        if (this.accepting && slot.binding !== undefined && slot.running.size === 0) this.next(slot);
      },
    );
  }

  /**
   * Runs `task` as a task of the root; a failure other than an interruption
   * goes to the supervisor. `started` sees the fiber before it can end, `ended` after.
   */
  fork(task: Task, started?: (fiber: Fiber) => void, ended?: (fiber: Fiber) => void): Fiber {
    const fiber = fork(task, this.ctx);
    started?.(fiber);
    this.tasks.add(fiber);
    fiber.observe((exit) => {
      this.tasks.delete(fiber);
      // an interruption is the policy's or the root's own doing
      if (exit.cause !== undefined && !hasInterruptsOnly(exit.cause)) this.supervise(exit.cause);
      ended?.(fiber);
    });
    return fiber;
  }

  /** An event a full policy rejects: an infrastructure defect, not an error of `e`. */
  private overflow(message: string): void {
    this.supervise(die(new Error(`event rejected: ${message}`)));
  }

  private supervise(cause: Cause): void {
    const fiber = fork(() => exitOf(() => this.supervisor(new Failure(cause))), this.ctx);
    this.supervisors.add(fiber);
    fiber.observe((exit) => {
      this.supervisors.delete(fiber);
      const supervised = exit.cause === undefined ? asExit(exit.value).cause : undefined;
      if (supervised !== undefined) this.broken(supervised);
    });
  }
}
