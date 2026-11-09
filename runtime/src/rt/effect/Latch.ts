// The bindings of `Latch` on the kernel: a gate fibers wait at while it is closed.
import { Suspend, type Run } from "../fiber.ts";

export class Latch {
  isOpen: boolean;
  readonly waiters = new Set<() => void>();
  constructor(open: boolean) {
    this.isOpen = open;
  }
}

export const make = (open = false): Latch => new Latch(open);

const wakeAll = (self: Latch): void => {
  const waiters = [...self.waiters];
  self.waiters.clear();
  for (const wake of waiters) wake();
};

const openIt = (self: Latch): boolean => {
  if (self.isOpen) return false;
  self.isOpen = true;
  wakeAll(self);
  return true;
};
export { openIt as open };

export const release = (self: Latch): boolean => {
  if (self.isOpen) return false;
  wakeAll(self);
  return true;
};

const closeIt = (self: Latch): boolean => {
  if (!self.isOpen) return false;
  self.isOpen = false;
  return true;
};
export { closeIt as close };

const wait = (self: Latch): Suspend =>
  new Suspend((resolve) => {
    if (self.isOpen) {
      resolve(undefined);
      return undefined;
    }
    const wake = (): void => resolve(undefined);
    self.waiters.add(wake);
    return () => self.waiters.delete(wake);
  });
export { wait as await };

export const whenOpen =
  (self: Latch) =>
  (task: () => unknown): Run =>
    (function* (): Run {
      yield wait(self);
      return task();
    })();

export const isOpen = (self: Latch): boolean => self.isOpen;
