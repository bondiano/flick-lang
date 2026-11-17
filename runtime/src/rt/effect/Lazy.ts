// The bindings of `Lazy` on the kernel: a computation that the first fiber to
// force it runs in itself, at most one attempt at a time, whose outcome every
// later force gets. A typed failure or a defect is an outcome; an interruption
// of the fiber running the attempt, or an unwind to one of its handlers, is
// not: the cell is pending again, and a waiter retries it.
import { type Exit, causeOf, failed, hasInterrupts, succeeded, unwinds } from "../cause.ts";
import { type Fiber, type Run, Suspend, currentFiber, raiseOf } from "../fiber.ts";

type Task = () => unknown;

export class LazyCell {
  /** The initializer, until an attempt settles the cell. */
  task: Task | undefined;
  /** The fiber running the attempt, while one runs. */
  owner: Fiber | undefined = undefined;
  /** How the attempt that settled the cell ended: final. */
  exit: Exit | undefined = undefined;
  readonly waiters = new Set<(value: unknown) => void>();
  constructor(task: Task) {
    this.task = task;
  }
}

/** `lazy task`: a pending cell; nothing runs. */
export const lazyMake = (task: Task): LazyCell => new LazyCell(task);

const SELF_FORCE = "lazy forced during its own initialization";

/** `await cell`: its outcome, running the initializer in this fiber when no attempt has settled it. */
export function* lazyForce(cell: LazyCell): Run {
  const fiber = currentFiber();
  for (;;) {
    if (cell.exit !== undefined) return raiseOf(cell.exit);
    if (cell.owner === undefined) break;
    if (cell.owner === fiber) throw new Error(SELF_FORCE);
    yield new Suspend((resolve) => {
      cell.waiters.add(resolve);
      return () => cell.waiters.delete(resolve);
    });
  }
  // claimed in the step that saw it pending, so no other fiber starts an attempt
  cell.owner = fiber;
  let exit: Exit | undefined;
  try {
    // oxlint-disable-next-line typescript/no-non-null-assertion -- a cell without an exit keeps its task
    const value: unknown = yield cell.task!();
    exit = succeeded(value);
    return value;
  } catch (thrown) {
    const cause = causeOf(thrown);
    if (!hasInterrupts(cause) && !unwinds(cause)) exit = failed(cause);
    throw thrown;
  } finally {
    cell.owner = undefined;
    if (exit !== undefined) {
      cell.exit = exit;
      cell.task = undefined;
    }
    // last: a waiter woken here runs on this stack and may start the next attempt
    const waiters = [...cell.waiters];
    cell.waiters.clear();
    for (const wake of waiters) wake(undefined);
  }
}
