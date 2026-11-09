// The bindings of `FiberSet` on the kernel: children of the owner of its region,
// not of its `K`. The owner's closing stops the group in a hook, before any
// finalizer. An open group takes a child whatever its owner's phase: only the
// group's own close refuses, with an interrupted fiber whose body does not run.
import { interrupt, raise } from "../cause.ts";
import { type Fiber, type Run, Suspend, asExit, awaitFiber, currentCtx, currentFiber, fork, forkNow, owned, raiseOf } from "../fiber.ts";
import { FlickFiber, type Owner } from "../owner.ts";
import { type Exit, hasInterruptsOnly, succeeded } from "../cause.ts";

/** The fiber a group stops its children as: their stopping fails no `join`. */
const INTERNAL = -2;

export class FiberSet {
  readonly owner: Owner | undefined;
  readonly fibers = new Set<Fiber>();
  closed = false;
  ended: Exit | undefined = undefined;
  readonly joining = new Set<(exit: Exit) => void>();
  readonly empty = new Set<() => void>();
  constructor(owner: Owner | undefined) {
    this.owner = owner;
  }
}

const end = (self: FiberSet, exit: Exit): void => {
  if (self.ended !== undefined) return;
  self.ended = exit;
  const joining = [...self.joining];
  self.joining.clear();
  for (const wake of joining) wake(exit);
};

function* clearIt(self: FiberSet): Run {
  const fibers = [...self.fibers];
  for (const f of fibers) f.interrupt(INTERNAL);
  for (const f of fibers) yield awaitFiber(f);
  return undefined;
}
export { clearIt as clear };

function* stop(self: FiberSet): Run {
  self.closed = true;
  yield clearIt(self);
  end(self, succeeded(undefined));
  return undefined;
}

export const flickMake = (): FiberSet => {
  const owner = currentCtx().owner;
  const self = new FiberSet(owner);
  owner?.closing.add(() => stop(self));
  return self;
};

const track = (self: FiberSet, fiber: Fiber, propagate: boolean): void => {
  if (fiber.exit === undefined) self.fibers.add(fiber);
  fiber.observe((exit) => {
    self.fibers.delete(fiber);
    const internal =
      exit.cause !== undefined &&
      hasInterruptsOnly(exit.cause) &&
      exit.cause.reasons.every((r) => r._tag === "Interrupt" && r.fiberId === INTERNAL);
    if (exit.cause !== undefined && !internal && (propagate || !hasInterruptsOnly(exit.cause))) end(self, exit);
    if (self.fibers.size === 0) {
      const empty = [...self.empty];
      self.empty.clear();
      for (const wake of empty) wake();
    }
  });
};

export const flickAdd =
  (propagateInterruption: boolean) =>
  (fiber: FlickFiber) =>
  (self: FiberSet): void => {
    if (self.closed) fiber.fiber.interrupt(INTERNAL);
    else track(self, fiber.fiber, propagateInterruption);
  };

interface RunOptions {
  readonly propagateInterruption: boolean;
  readonly startImmediately: boolean;
}

export const flickRun =
  (options: RunOptions) =>
  (task: () => unknown) =>
  (self: FiberSet): FlickFiber => {
    const ctx = self.owner === undefined ? currentCtx() : owned(currentCtx(), self.owner);
    if (self.closed) {
      return new FlickFiber(forkNow(() => raise(interrupt(currentFiber().id)), ctx));
    }
    const fiber = options.startImmediately ? forkNow(task, ctx) : fork(task, ctx);
    track(self, fiber, options.propagateInterruption);
    return new FlickFiber(fiber);
  };

export const size = (self: FiberSet): number => self.fibers.size;

function* joinIt(self: FiberSet): Run {
  if (self.ended !== undefined) return raiseOf(self.ended);
  const exit = asExit(
    yield new Suspend((resolve) => {
      self.joining.add(resolve);
      return () => self.joining.delete(resolve);
    }),
  );
  return raiseOf(exit);
}
export { joinIt as join };

export const awaitEmpty = (self: FiberSet): Suspend =>
  new Suspend((resolve) => {
    if (self.fibers.size === 0) {
      resolve(undefined);
      return undefined;
    }
    const wake = (): void => resolve(undefined);
    self.empty.add(wake);
    return () => self.empty.delete(wake);
  });
