// The bindings of `Deferred` on the kernel: a value that many wait for, set once.
import { none, type Option, some } from "../core/data.ts";
import { type Exit, die, fail, failed, interrupt, succeeded } from "../cause.ts";
import { type Run, Suspend, asExit, currentFiber, exitOf, raiseOf } from "../fiber.ts";
import type { Failure } from "../outcome.ts";

/** What a set deferred gives each waiter: the task it was set to, which each runs, or one of how it ended. */
type Task = () => unknown;

export class Deferred {
  task: Task | undefined = undefined;
  readonly waiters = new Set<(task: Task) => void>();
}

export const make = (): Deferred => new Deferred();

const setTo = (self: Deferred, task: Task): boolean => {
  if (self.task !== undefined) return false;
  self.task = task;
  const waiters = [...self.waiters];
  self.waiters.clear();
  for (const wake of waiters) wake(task);
  return true;
};

const settle = (self: Deferred, exit: Exit): boolean => setTo(self, () => raiseOf(exit));

/** Waits for it to be set: its value or failure. */
function* awaited(self: Deferred): Run {
  if (self.task !== undefined) return self.task();
  const task = yield new Suspend((resolve) => {
    self.waiters.add(resolve);
    return () => self.waiters.delete(resolve);
  });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a waiter is woken with the task
  return (task as Task)();
}
export { awaited as await };

export const succeed =
  (value: unknown) =>
  (self: Deferred): boolean =>
    settle(self, succeeded(value));
export const sync =
  (f: (unit: undefined) => unknown) =>
  (self: Deferred): boolean =>
    settle(self, succeeded(f(undefined)));
const failWith =
  (error: unknown) =>
  (self: Deferred): boolean =>
    settle(self, failed(fail(error)));
export { failWith as fail };
export const failSync =
  (f: (unit: undefined) => unknown) =>
  (self: Deferred): boolean =>
    settle(self, failed(fail(f(undefined))));
export const flickDone =
  (exit: Exit) =>
  (self: Deferred): boolean =>
    settle(self, exit);
export const flickFailCause =
  (failure: Failure) =>
  (self: Deferred): boolean =>
    settle(self, failed(failure.cause));
export const flickFailCauseSync =
  (f: (unit: undefined) => Failure) =>
  (self: Deferred): boolean =>
    settle(self, failed(f(undefined).cause));
const interruptIt = (self: Deferred): boolean => settle(self, failed(interrupt(currentFiber().id)));
export { interruptIt as interrupt };
export const interruptWith =
  (fiberId: number) =>
  (self: Deferred): boolean =>
    settle(self, failed(interrupt(fiberId)));
export const isDone = (self: Deferred): boolean => self.task !== undefined;
export const poll = (self: Deferred): Option<() => unknown> => (self.task === undefined ? none : some(self.task));
export const complete =
  (task: () => unknown) =>
  (self: Deferred): Run =>
    (function* (): Run {
      if (self.task !== undefined) return false;
      return settle(self, asExit(yield exitOf(task)));
    })();
/** Sets it to `task`, which each waiter runs. */
export const completeWith =
  (task: () => unknown) =>
  (self: Deferred): boolean =>
    setTo(self, task);
export const defectOf = (message: string): Exit => failed(die(message));
