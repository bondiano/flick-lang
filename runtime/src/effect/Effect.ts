// The bindings of `Effect` on the kernel: what the runtime contract's cases run.
import type { Data } from "../core/data.ts";
import { allOf } from "../concurrency.ts";
import { sleep } from "../duration.ts";
import { type Exit, die as dieCause, hasInterrupts, interrupt, isInterrupt, raise, succeeded, unwinds } from "../cause.ts";
import {
  type Run,
  Suspend,
  asExit,
  atMask,
  caught,
  currentFiber,
  exitOf,
  raiseOf,
  uninterruptible as masked,
  yieldNow,
} from "../fiber.ts";
import { Failure } from "../outcome.ts";
import { finalize } from "../owner.ts";

export { sleep };

/** `Effect.void`. */
const nothing = (): undefined => undefined;
export { nothing as void };

export const die = (message: string): never => raise(dieCause(message));
export const flickNever = (): Suspend => new Suspend(() => undefined);
export const flickYieldNow = (): unknown => yieldNow();
export const flickFiberId = (): number => currentFiber().id;
export const flickInterrupt = (): never => raise(interrupt(currentFiber().id));

/** How `task` ended, as a value, unless it unwinds to a handler of effects. */
export function* flickExit(task: () => unknown): Run {
  const exit = asExit(yield caught(task));
  if (exit.cause !== undefined && unwinds(exit.cause)) return raiseOf(exit);
  return exit;
}

export const uninterruptible = (task: () => unknown): Run => masked(task);
export const interruptible = (task: () => unknown): Run => atMask(0, task);

/** `f restore`, one mask deeper; `restore` gives a task the depth around the call. */
export const uninterruptibleMask = (f: (restore: (task: () => unknown) => () => unknown) => () => unknown): Run => {
  const depth = currentFiber().mask;
  return masked(() => f((task) => () => atMask(depth, task))());
};

export const interruptibleMask = (f: (restore: (task: () => unknown) => () => unknown) => () => unknown): Run => {
  const depth = currentFiber().mask;
  return atMask(0, () => f((task) => () => atMask(depth, task))());
};

/** `finalizer` runs, masked, however `task` ended; the failures of both join. */
function* finally_(task: () => unknown, finalizer: (exit: Exit) => unknown): Run {
  const fiber = currentFiber();
  const depth = fiber.mask;
  fiber.mask = depth + 1;
  try {
    const exit = asExit(yield exitOf(() => atMask(depth, task)));
    const ended = asExit(yield exitOf(() => finalizer(exit)));
    return raiseOf(finalize(exit, ended));
  } finally {
    fiber.mask = depth;
  }
}

export const ensuring =
  (finalizer: () => unknown) =>
  (task: () => unknown): Run =>
    finally_(task, () => finalizer());

export const flickOnExit =
  (f: (exit: Exit) => unknown) =>
  (task: () => unknown): Run =>
    finally_(task, f);

export const flickOnInterrupt =
  (f: (interruptors: ReadonlyArray<number>) => unknown) =>
  (task: () => unknown): Run =>
    finally_(task, (exit) =>
      exit.cause !== undefined && hasInterrupts(exit.cause)
        ? f(exit.cause.reasons.flatMap((r) => (isInterrupt(r) && r.fiberId !== undefined ? [r.fiberId] : [])))
        : undefined,
    );

export const flickMatchCause =
  (onFailure: (failure: Failure) => unknown, onSuccess: (value: unknown) => unknown) =>
  (task: () => unknown): Run =>
    (function* (): Run {
      const exit = asExit(yield caught(task));
      if (exit.cause === undefined) return onSuccess(exit.value);
      if (unwinds(exit.cause)) return raiseOf(exit);
      return onFailure(new Failure(exit.cause));
    })();

const limitOf = (c: Data): number => {
  const name = c._tag;
  if (name.includes("Sequential")) return 1;
  if (name.includes("Unbounded")) return Infinity;
  const n: unknown = c["_0"];
  return typeof n === "number" ? n : Infinity;
};

export const flickAll =
  (c: Data) =>
  (tasks: ReadonlyArray<() => unknown>): Run =>
    allOf(tasks, limitOf(c));

export const flickSucceedExit = succeeded;

export { repeat, repeatOrElse, retry, retryOrElse, schedule, scheduleFrom } from "../schedule.ts";
