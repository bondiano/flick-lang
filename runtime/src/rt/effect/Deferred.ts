// The bindings of `Deferred` on the kernel: a value that many wait for, set once.
import { none, type Option, some } from "../../core/data.ts";
import { type Exit, die, fail, failed, interrupt, succeeded } from "../cause.ts";
import { type Run, Suspend, asExit, currentFiber, exitOf, raiseOf } from "../fiber.ts";
import type { Failure } from "../outcome.ts";

export class Deferred {
  exit: Exit | undefined = undefined;
  readonly waiters = new Set<(exit: Exit) => void>();
}

export const make = (): Deferred => new Deferred();

const settle = (self: Deferred, exit: Exit): boolean => {
  if (self.exit !== undefined) return false;
  self.exit = exit;
  const waiters = [...self.waiters];
  self.waiters.clear();
  for (const wake of waiters) wake(exit);
  return true;
};

/** Waits for it to be set: its value or failure. */
function* awaited(self: Deferred): Run {
  if (self.exit !== undefined) return raiseOf(self.exit);
  const exit = asExit(
    yield new Suspend((resolve) => {
      self.waiters.add(resolve);
      return () => self.waiters.delete(resolve);
    }),
  );
  return raiseOf(exit);
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
export const isDone = (self: Deferred): boolean => self.exit !== undefined;
export const poll = (self: Deferred): Option<() => unknown> => {
  const { exit } = self;
  return exit === undefined ? none : some(() => raiseOf(exit));
};
export const complete =
  (task: () => unknown) =>
  (self: Deferred): Run =>
    (function* (): Run {
      if (self.exit !== undefined) return false;
      return settle(self, asExit(yield exitOf(task)));
    })();
export const completeWith =
  (task: () => unknown) =>
  (self: Deferred): boolean =>
    settle(self, succeeded(task));
export const defectOf = (message: string): Exit => failed(die(message));
