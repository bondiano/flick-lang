// Callbacks JS calls that run Flick tasks: Promise callbacks and scoped
// event listeners. Both belong to an owner: they run its children with the
// services of where they were made, and stop with it.
import { Cause, type Context, Effect, Exit, type Fiber, Result } from "effect";

import { Failure, describeCause } from "../cause.ts";
import { applyRuntime } from "../effects.ts";
import { CurrentOwner, type Owner, within, withOwner } from "../owner.ts";

/** The rejection of a Promise callback or façade: a safe, inert report. */
export class RuntimeFailure extends Error {
  constructor(cause: Cause.Cause<unknown>) {
    super(describeCause(cause));
    this.name = "RuntimeFailure";
  }
}

/** Runs `effect` as a child of `owner` with `context`; its fiber. */
const runChild = <A, E, R>(
  owner: Owner,
  context: Context.Context<R>,
  effect: Effect.Effect<A, E, R>,
): Fiber.Fiber<A, E> => {
  const fiber = Effect.runFork(Effect.provideContext(effect, context));
  owner.children.add(fiber);
  fiber.addObserver(() => owner.children.delete(fiber));
  return fiber;
};

const currentOwner = /*#__PURE__*/ Effect.flatMap(Effect.service(CurrentOwner), (owner) =>
  owner === undefined ? Effect.die(new Error("a callback needs an owner: run it inside `main` or a scope")) : Effect.succeed(owner),
);

/** `Js.promise_callback f`: `(a) => Promise<b>` running `f a` in this owner. */
export const promiseCallback = <A, B, E, R>(
  f: (a: A) => Effect.Effect<B, E, R>,
): Effect.Effect<(a: A) => Promise<B>, never, R> =>
  Effect.gen(function* () {
    const owner = yield* currentOwner;
    const context = yield* Effect.context<R>();
    return (a: A) => {
      if (!owner.open) {
        return Promise.reject(new RuntimeFailure(Cause.interrupt()));
      }
      const fiber = runChild(owner, context, Effect.suspend(() => f(a)));
      return new Promise<B>((resolve, reject) =>
        fiber.addObserver((exit) =>
          Exit.isSuccess(exit) ? resolve(exit.value) : reject(new RuntimeFailure(exit.cause)),
        ),
      );
    };
  });

/** The state of a listener, kept on its function: the ABI of a listener is the function. */
const STATE = /*#__PURE__*/ Symbol("flick/listener");

interface ListenerState<A> {
  closed: boolean;
  /** Each event boxed: an event may be `undefined` itself. */
  readonly queue: Array<{ readonly event: A }>;
  wake: (() => void) | undefined;
}

/** `Listener a`: the function an event source calls. */
export type Listener<A> = ((event: A) => void) & { readonly [STATE]: ListenerState<A> };

/** `Events.queue`: how many events may wait. */
export interface QueuePolicy {
  readonly capacity: number;
}

/**
 * `Events.listen owner policy supervisor handler`: a JS function queueing
 * each event for a worker that runs `handler` one event at a time as a
 * child of `owner`. A full queue drops the event and tells `supervisor`.
 */
export const eventsListen = <A, R, R2>(
  owner: Owner,
  policy: QueuePolicy,
  supervisor: (failure: Failure) => Effect.Effect<unknown, unknown, R2>,
  handler: (event: A) => Effect.Effect<unknown, unknown, R>,
): Effect.Effect<Listener<A>, never, R | R2> =>
  Effect.gen(function* () {
    const context = yield* Effect.context<R | R2>();
    const supervise = (cause: Cause.Cause<unknown>): void => {
      const run = Effect.exit(Effect.suspend(() => within(owner, supervisor(new Failure(cause)))));
      const fiber = runChild(owner, context, run);
      fiber.addObserver((exit) => {
        if (Exit.isSuccess(exit) && Exit.isFailure(exit.value)) owner.defects.push(exit.value.cause);
      });
    };
    const state: ListenerState<A> = { closed: false, queue: [], wake: undefined };
    const worker = Effect.gen(function* () {
      for (;;) {
        const next = state.queue.shift();
        if (next === undefined) {
          yield* Effect.callback<void>((resume) => {
            state.wake = () => resume(Effect.void);
          });
          state.wake = undefined;
          continue;
        }
        const exit = yield* Effect.exit(Effect.suspend(() => within(owner, handler(next.event))));
        if (Exit.isFailure(exit) && !Cause.hasInterruptsOnly(exit.cause)) supervise(exit.cause);
      }
    });
    runChild(owner, context, worker);
    const listener = Object.assign(
      (event: A): void => {
        if (state.closed || !owner.open) return;
        if (state.queue.length >= policy.capacity) {
          supervise(Cause.die(new Error(`event queue full: ${policy.capacity} events are waiting`)));
          return;
        }
        state.queue.push({ event });
        // the handler never runs inside the call of the event source
        queueMicrotask(() => state.wake?.());
      },
      { [STATE]: state },
    );
    owner.closing.add(() => {
      state.closed = true;
      return Effect.void;
    });
    return listener;
  });

/** `Events.attach listener add remove`: `remove` gets the function `add` got. */
export const eventsAttach = <A, E, R, E2, R2>(
  listener: Listener<A>,
  add: (listener: Listener<A>) => Effect.Effect<unknown, E, R>,
  remove: (listener: Listener<A>) => Effect.Effect<unknown, E2, R2>,
): Effect.Effect<void, E, R | R2> =>
  Effect.gen(function* () {
    const owner = yield* currentOwner;
    // `remove` runs when the owner closes, with the services of where it was attached
    const context = yield* Effect.context<R2>();
    yield* add(listener);
    owner.closing.add(() => {
      listener[STATE].closed = true;
      return Effect.provideContext(
        Effect.suspend(() => remove(listener)),
        context,
      );
    });
  });

/** The last argument of a Promise façade call. */
export interface FacadeOptions {
  readonly signal?: AbortSignal;
}

const signalOf = (options: unknown): AbortSignal | undefined => {
  const signal: unknown = typeof options === "object" && options !== null ? Reflect.get(options, "signal") : undefined;
  return signal instanceof AbortSignal ? signal : undefined;
};

/**
 * The Promise façade of a runtime function taking `arity` arguments: each
 * call runs it under an owner of its own. An expected failure resolves to
 * `Result.fail`, a defect or an interruption rejects with a
 * `RuntimeFailure`; `options.signal` interrupts the run, and the promise
 * settles only after cleanup.
 */
export const promiseFacade =
  (fn: unknown, arity: number) =>
  (...args: ReadonlyArray<unknown>): Promise<Result.Result<unknown, unknown>> => {
    const signal = signalOf(args[arity]);
    if (signal?.aborted === true) return Promise.reject(new RuntimeFailure(Cause.interrupt()));
    const run = withOwner(() =>
      Effect.suspend(
        () =>
          // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiler makes a façade only for exports that need no services
          applyRuntime(fn, args.slice(0, arity)) as Effect.Effect<unknown, unknown>,
      ),
    );
    const fiber = Effect.runFork(run, { signal });
    return new Promise((resolve, reject) =>
      fiber.addObserver((exit) => {
        if (Exit.isSuccess(exit)) return resolve(Result.succeed(exit.value));
        const { cause } = exit;
        const fail = Cause.hasDies(cause) ? undefined : cause.reasons.find(Cause.isFailReason);
        return fail ? resolve(Result.fail(fail.error)) : reject(new RuntimeFailure(cause));
      }),
    );
  };
