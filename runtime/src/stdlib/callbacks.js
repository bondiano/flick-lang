// Callbacks JS calls that run Flick tasks: Promise callbacks and scoped
// event listeners. Both belong to an owner: they run its children with the
// services of where they were made, and stop with it.
import { Cause, Effect, Exit, Fiber, Result } from "effect";

import { Failure, describeCause } from "../cause.js";
import { CurrentOwner, reportDefect, withOwner } from "../owner.js";

/** The rejection of a Promise callback or façade: a safe, inert report. */
export class RuntimeFailure extends Error {
  constructor(cause) {
    super(describeCause(cause));
    this.name = "RuntimeFailure";
  }
}

/** Runs `effect` as a child of `owner` with `context`; its exit, as a promise. */
const runChild = (owner, context, effect) => {
  const fiber = Effect.runFork(Effect.provideContext(effect, context));
  owner.children.add(fiber);
  fiber.addObserver(() => owner.children.delete(fiber));
  return fiber;
};

/** `Js.promise_callback f`: `(a) => Promise<b>` running `f a` in this owner. */
export const promiseCallback = (f) =>
  Effect.gen(function* () {
    const owner = yield* Effect.service(CurrentOwner);
    const context = yield* Effect.context();
    return (a) => {
      if (!owner.open) {
        return Promise.reject(new RuntimeFailure(Cause.interrupt()));
      }
      const fiber = runChild(owner, context, Effect.suspend(() => f(a)));
      return new Promise((resolve, reject) =>
        fiber.addObserver((exit) =>
          Exit.isSuccess(exit) ? resolve(exit.value) : reject(new RuntimeFailure(exit.cause)),
        ),
      );
    };
  });

/** The state of a listener, kept on its function: the ABI of a listener is the function. */
const STATE = Symbol("flick/listener");

/**
 * `Events.listen owner policy supervisor handler`: a JS function queueing
 * each event for a worker that runs `handler` one event at a time as a
 * child of `owner`. A full queue drops the event and tells `supervisor`.
 */
export const eventsListen = (owner, policy, supervisor, handler) =>
  Effect.gen(function* () {
    const context = yield* Effect.context();
    const supervise = (cause) => {
      const run = Effect.exit(Effect.suspend(() => supervisor(new Failure(cause))));
      const fiber = runChild(owner, context, run);
      fiber.addObserver((exit) => {
        if (Exit.isSuccess(exit) && Exit.isFailure(exit.value)) reportDefect(exit.value.cause);
      });
    };
    const state = { closed: false, queue: [], wake: undefined, worker: undefined };
    const worker = Effect.gen(function* () {
      for (;;) {
        if (state.queue.length === 0) {
          yield* Effect.callback((resume) => {
            state.wake = () => resume(Effect.void);
          });
          state.wake = undefined;
          continue;
        }
        const event = state.queue.shift();
        const exit = yield* Effect.exit(Effect.suspend(() => handler(event)));
        if (Exit.isFailure(exit) && !Cause.hasInterruptsOnly(exit.cause)) supervise(exit.cause);
      }
    });
    state.worker = runChild(owner, context, worker);
    const fn = (event) => {
      if (state.closed || !owner.open) return;
      if (state.queue.length >= policy.capacity) {
        supervise(Cause.die(new Error(`event queue full: ${policy.capacity} events are waiting`)));
        return;
      }
      state.queue.push(event);
      // the handler never runs inside the call of the event source
      queueMicrotask(() => state.wake?.());
    };
    owner.closing.add(() => {
      state.closed = true;
      return Effect.void;
    });
    fn[STATE] = state;
    return fn;
  });

/** `Events.attach listener add remove`: `remove` gets the function `add` got. */
export const eventsAttach = (listener, add, remove) =>
  Effect.gen(function* () {
    const owner = yield* Effect.service(CurrentOwner);
    yield* add(listener);
    owner.closing.add(() => {
      listener[STATE].closed = true;
      return Effect.suspend(() => remove(listener));
    });
  });

/**
 * The Promise façade of a runtime function taking `arity` arguments: each
 * call runs it under an owner of its own. An expected failure resolves to
 * `Result.fail`, a defect or an interruption rejects with a
 * `RuntimeFailure`; `options.signal` interrupts the run, and the promise
 * settles only after cleanup.
 */
export const promiseFacade =
  (fn, arity) =>
  (...args) => {
    const options = args[arity];
    const run = withOwner(() => Effect.suspend(() => args.slice(0, arity).reduce((f, a) => f(a), fn)));
    const fiber = Effect.runFork(run);
    options?.signal?.addEventListener("abort", () => Effect.runFork(Fiber.interrupt(fiber)), { once: true });
    return new Promise((resolve, reject) =>
      fiber.addObserver((exit) => {
        if (Exit.isSuccess(exit)) return resolve(Result.succeed(exit.value));
        const { cause } = exit;
        const fail = Cause.hasDies(cause) ? undefined : cause.reasons.find(Cause.isFailReason);
        return fail ? resolve(Result.fail(fail.error)) : reject(new RuntimeFailure(cause));
      }),
    );
  };
