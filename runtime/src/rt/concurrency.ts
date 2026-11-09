// Structured concurrency: branches run as children of the combinator, which
// owns them. Each starts at once on this stack, up to where it first suspends.
// On a failure no new branch starts, the running ones are asked to stop and
// awaited, and what they fail with besides that joins the failure.
import { type Option, err, none, ok, some } from "../core/data.ts";
import { tag } from "../tags.ts";
import { type Exit, type Reason, combine, elect, fail, failed, fromReasons, hasDies, isFail, isInterrupt, raise, succeeded, unwinds } from "./cause.ts";
import { type Fiber, type Run, Suspend, asExit, awaitFiber, caught, currentCtx, currentFiber, forkNow, raiseOf } from "./fiber.ts";
import { Duration, sleep } from "./duration.ts";

type Task = () => unknown;

const ignore = (): void => {};

function* stopAll(fibers: Iterable<Fiber>, by: number): Run {
  const running = [...fibers];
  for (const f of running) f.interrupt(by);
  for (const f of running) yield awaitFiber(f);
  return undefined;
}

/** Runs `tasks`, at most `limit` at a time: their values in order, or the failure. */
function* all(tasks: ReadonlyArray<Task>, limit: number): Run {
  const parent = currentFiber();
  const ctx = currentCtx();
  const out: Array<unknown> = Array.from({ length: tasks.length });
  const fibers = new Set<Fiber>();
  let index = 0;
  let done = false;
  let paused = false;
  let terminal: Exit | undefined;
  let resume: (value: unknown) => void = ignore;
  const step = (exit: Exit, i: number): Exit | undefined => {
    if (exit.cause !== undefined) return exit;
    out[i] = exit.value;
    return undefined;
  };
  const go = (): Exit | undefined => {
    paused = false;
    for (; terminal === undefined && index < tasks.length; index++) {
      const i = index;
      const task = tasks[i];
      if (task === undefined) continue;
      const fiber = forkNow(task, ctx, parent.mask > 0 ? 1 : 0);
      if (fiber.exit !== undefined) {
        terminal = step(fiber.exit, i);
        if (terminal !== undefined) break;
        continue;
      }
      fibers.add(fiber);
      fiber.observe((exit) => {
        fibers.delete(fiber);
        if (terminal !== undefined) {
          if (exit.cause !== undefined) {
            const reasons: Array<Reason> = exit.cause.reasons.filter((r) => !isInterrupt(r));
            if (reasons.length > 0) {
              const cause = fromReasons(reasons);
              terminal = failed(terminal.cause === undefined ? cause : elect(combine(terminal.cause, cause)));
            }
          }
        } else {
          const result = step(exit, i);
          if (result !== undefined) {
            terminal = result;
            go();
          }
        }
        if (paused) {
          const ended = go();
          if (ended !== undefined) resume(ended);
        } else if (done && fibers.size === 0) {
          resume(terminal ?? succeeded(undefined));
        }
      });
      if (fibers.size < limit) continue;
      paused = true;
      index++;
      return undefined;
    }
    done = true;
    if (terminal !== undefined) {
      if (fibers.size > 0) {
        for (const f of fibers) f.interrupt(parent.id);
        return undefined;
      }
      return terminal;
    }
    return fibers.size === 0 ? succeeded(undefined) : undefined;
  };
  try {
    const exit = asExit(
      yield new Suspend((resolve) => {
        resume = resolve;
        const ended = go();
        if (ended !== undefined) resolve(ended);
      }),
    );
    raiseOf(exit);
    return out;
  } catch (thrown) {
    if (fibers.size === 0) throw thrown;
    // interrupted while the branches run: they stop before this ends
    parent.mask++;
    try {
      terminal ??= succeeded(undefined);
      yield stopAll(fibers, parent.id);
    } finally {
      parent.mask--;
    }
    if (terminal.cause !== undefined) raiseOf(terminal);
    throw thrown;
  }
}

/** `parallel a b`: both, as a tuple. */
export const parallel = (a: Task, b: Task): Run => all([a, b], Infinity);

/** `parallel_all limit tasks`: at most `limit` branches at a time. */
export const parallelAll = (limit: number, tasks: ReadonlyArray<Task>): Run => {
  if (!Number.isInteger(limit) || limit <= 0) {
    throw new RangeError(`parallel_all needs a positive finite integer limit, not ${String(limit)}`);
  }
  return all(tasks, limit);
};

/** All of them at once, at most `limit`. */
export const allOf = (tasks: ReadonlyArray<Task>, limit: number): Run => all(tasks, limit);

function* asResult(task: Task): Run {
  const exit = asExit(yield caught(task));
  if (exit.cause === undefined) return ok(exit.value);
  const reason = hasDies(exit.cause) ? undefined : exit.cause.reasons.find(isFail);
  return reason === undefined ? raiseOf(exit) : err(reason.error);
}

/** `parallel_results`: each branch's typed failure as an `Err`; a defect or an interruption still stops the group. */
export const parallelResults = (tasks: ReadonlyArray<Task>): Run => all(tasks.map((t) => () => asResult(t)), Infinity);

function* asExitValue(task: Task): Run {
  const exit = asExit(yield caught(task));
  if (exit.cause !== undefined && unwinds(exit.cause)) return raiseOf(exit);
  return exit;
}

/** `parallel_exits`: how each branch ended. */
export const parallelExits = (tasks: ReadonlyArray<Task>): Run => all(tasks.map((t) => () => asExitValue(t)), Infinity);

/** The first of `tasks` to end, or with `success` to succeed; the losers stopped and awaited. */
function* race(tasks: ReadonlyArray<Task>, success: boolean): Run {
  const parent = currentFiber();
  const ctx = currentCtx();
  const fibers = new Set<Fiber>();
  const failures: Array<Reason> = [];
  try {
    const exit = asExit(
      yield new Suspend((resolve) => {
        let settled = false;
        let ended = 0;
        for (const task of tasks) {
          if (settled) break;
          const fiber = forkNow(task, ctx);
          fibers.add(fiber);
          fiber.observe((e) => {
            fibers.delete(fiber);
            ended++;
            if (success && e.cause !== undefined) {
              failures.push(...e.cause.reasons);
              if (ended >= tasks.length) resolve(failed(elect(fromReasons(failures))));
              return;
            }
            settled = true;
            resolve(e);
          });
        }
      }),
    );
    return raiseOf(exit);
  } finally {
    if (fibers.size > 0) {
      parent.mask++;
      try {
        yield stopAll(fibers, parent.id);
      } finally {
        parent.mask--;
      }
    }
  }
}

/** `race_success`: the first branch to succeed. */
export const raceSuccess = (a: Task, b: Task): Run => race([a, b], true);

/** `race_first`: the first branch to end, however it ends. */
export const raceFirst = (a: Task, b: Task): Run => race([a, b], false);

const TIMEOUT_ERROR = /*#__PURE__*/ tag("Prelude.TimeoutError.TimeoutError");

/** `timeout millis task`: fails with `TimeoutError` when time runs out first. */
export const timeout = (millis: number, task: Task): Run =>
  race(
    [
      task,
      function* (): Run {
        yield sleep(new Duration(millis));
        return raise(fail({ _tag: TIMEOUT_ERROR }));
      },
    ],
    false,
  );

function* some_(task: Task): Run {
  return some(yield task());
}

/** `timeout_option millis task`: `None` when time runs out first. */
export const timeoutOption = (millis: number, task: Task): Run =>
  race(
    [
      () => some_(task),
      function* (): Run {
        yield sleep(new Duration(millis));
        const nothing: Option<never> = none;
        return nothing;
      },
    ],
    false,
  );
