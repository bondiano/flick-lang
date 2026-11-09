// A listener belongs to a lexical owner. Events wait in its bounded FIFO and
// one child handles them; closing the owner stops admission before detaching
// the source, then interrupts and waits for that child.
import { die, hasInterruptsOnly } from "./cause.ts";
import { type Ctx, type Run, Suspend, Under, asExit, currentCtx, exitOf, inCtx } from "./fiber.ts";
import { Failure } from "./outcome.ts";
import { type Owner, fiberSpawn, fiberSpawnNow, within } from "./owner.ts";

const STATE = /*#__PURE__*/ Symbol("flick/rt/listener");

interface ListenerState<A> {
  closed: boolean;
  readonly queue: Array<{ readonly event: A }>;
  wake: (() => void) | undefined;
}

export type Listener<A> = ((event: A) => void) & { readonly [STATE]: ListenerState<A> };

export interface QueuePolicy {
  readonly capacity: number;
}

export const eventsQueue = (capacity: number): QueuePolicy => {
  if (!Number.isInteger(capacity) || capacity <= 0) {
    throw new RangeError(`Events.queue needs a positive integer capacity, not ${String(capacity)}`);
  }
  return Object.freeze({ capacity });
};

const reportDefect = (owner: Owner) => (failure: Failure): void => {
  if (!failure.shutdown) owner.defects.push(failure.cause);
};

const supervise = (owner: Owner, ctx: Ctx, supervisor: (failure: Failure) => unknown, failure: Failure): void => {
  inCtx(ctx, () => fiberSpawnNow(owner, reportDefect(owner), () => supervisor(failure)));
};

/** `Events.listen`: the callback is the value the JS event source receives. */
export const eventsListen = <A>(
  owner: Owner,
  policy: QueuePolicy,
  supervisor: (failure: Failure) => unknown,
  handler: (event: A) => () => unknown,
): Listener<A> => {
  const ctx = currentCtx();
  const state: ListenerState<A> = { closed: false, queue: [], wake: undefined };
  function* worker(): Run {
    for (;;) {
      const next = state.queue.shift();
      if (next === undefined) {
        yield new Suspend((resolve) => {
          state.wake = () => resolve(undefined);
          return () => { state.wake = undefined; };
        });
        state.wake = undefined;
        continue;
      }
      const ended = asExit(yield exitOf(() => within(owner, () => handler(next.event)())));
      if (ended.cause !== undefined && !hasInterruptsOnly(ended.cause)) {
        const failure = new Failure(ended.cause);
        if (owner.open) supervise(owner, ctx, supervisor, failure);
        else yield supervisor(failure);
      }
    }
  }
  inCtx(ctx, () => fiberSpawn(owner, reportDefect(owner), worker));
  const listener: Listener<A> = Object.assign(
    (event: A): void => {
      if (state.closed || !owner.open) return;
      if (state.queue.length >= policy.capacity) {
        supervise(owner, ctx, supervisor, new Failure(die(new Error(`event queue full: ${policy.capacity} events are waiting`))));
        return;
      }
      state.queue.push({ event });
      // A source never runs the handler within its own call stack.
      queueMicrotask(() => state.wake?.());
    },
    { [STATE]: state },
  );
  owner.closing.add(() => { state.closed = true; });
  return listener;
};

/** `Events.attach`: a removal sees the evidence of its registration. */
export function* eventsAttach<A>(
  listener: Listener<A>,
  add: (listener: Listener<A>) => unknown,
  remove: (listener: Listener<A>) => unknown,
): Run {
  const ctx = currentCtx();
  const owner = ctx.owner;
  if (owner === undefined) throw new Error("an event listener needs an owner: run it inside `main` or a scope");
  yield add(listener);
  owner.closing.add(() => {
    listener[STATE].closed = true;
    return new Under(ctx, () => remove(listener));
  });
  return undefined;
}
