// `Lazy` on Effect: a computation that the first fiber to force it runs in
// itself, at most one attempt at a time, whose outcome every later force
// gets. A typed failure or a defect is an outcome; an interruption of the
// fiber running the attempt, or an unwind to one of its handlers, is not: the
// cell is pending again, and a waiter retries it. `Effect.cached` would keep
// the interruption as the outcome.
import { Cause, Deferred, Effect, Exit, type Fiber } from "effect";

import { unwinds } from "../cause.ts";

export class LazyCell<A, E, R> {
  /** The initializer, until an attempt settles the cell. */
  task: Effect.Effect<A, E, R> | undefined;
  /** The attempt that runs now: its fiber, and what is done when it ends, however it ends. */
  running: { readonly owner: Fiber.Fiber<unknown, unknown>; readonly ended: Deferred.Deferred<void> } | undefined = undefined;
  /** How the attempt that settled the cell ended: final. */
  exit: Exit.Exit<A, E> | undefined = undefined;
  constructor(task: Effect.Effect<A, E, R>) {
    this.task = task;
  }
}

/** `lazy task`: a pending cell; nothing runs. */
export const lazyMake = <A, E, R>(task: Effect.Effect<A, E, R>): Effect.Effect<LazyCell<A, E, R>> =>
  Effect.sync(() => new LazyCell(task));

const SELF_FORCE = "lazy forced during its own initialization";

const cached = (cause: Cause.Cause<unknown>): boolean => !Cause.hasInterrupts(cause) && !unwinds(cause);

/** `await cell`: its outcome, running the initializer in this fiber when no attempt has settled it. */
export const lazyForce = <A, E, R>(cell: LazyCell<A, E, R>): Effect.Effect<A, E, R> =>
  // the state is read and claimed in one step that no interruption splits
  Effect.uninterruptibleMask((restore) =>
    Effect.withFiber((fiber): Effect.Effect<A, E, R> => {
      if (cell.exit !== undefined) return cell.exit;
      const running = cell.running;
      if (running?.owner === fiber) return Effect.die(new Error(SELF_FORCE));
      if (running !== undefined) return restore(Effect.flatMap(Deferred.await(running.ended), () => lazyForce(cell)));
      // oxlint-disable-next-line typescript/no-non-null-assertion -- a cell without an exit keeps its task
      const task = cell.task!;
      const ended = Deferred.makeUnsafe<void>();
      cell.running = { owner: fiber, ended };
      return Effect.flatMap(Effect.exit(restore(task)), (exit) => {
        cell.running = undefined;
        if (Exit.isSuccess(exit) || cached(exit.cause)) {
          cell.exit = exit;
          cell.task = undefined;
        }
        Deferred.doneUnsafe(ended, Exit.void);
        return exit;
      });
    }),
  );
