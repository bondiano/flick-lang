// The Promise façade of a runtime function: what JavaScript calls to run it.
import { type Result, err, ok } from "../core/data.ts";
import { type Cause, interrupt, recoverable } from "./cause.ts";
import { describeReport, report } from "./describe.ts";
import { currentCtx, fork } from "./fiber.ts";
import { withOwner } from "./owner.ts";

/** The rejection of a façade: a safe, inert report of a defect or an interruption. */
export class RuntimeFailure extends Error {
  constructor(cause: Cause) {
    super(describeReport(report(cause)));
    this.name = "RuntimeFailure";
  }
}

const signalOf = (options: unknown): AbortSignal | undefined => {
  const signal: unknown = typeof options === "object" && options !== null ? Reflect.get(options, "signal") : undefined;
  return signal instanceof AbortSignal ? signal : undefined;
};

/** `f a` for a curried function of the program. */
const apply = (f: unknown, a: unknown): unknown => {
  if (typeof f !== "function") throw new TypeError("the function takes fewer arguments");
  const result: unknown = Reflect.apply(f, undefined, [a]);
  return result;
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
  (...args: ReadonlyArray<unknown>): Promise<Result<unknown, unknown>> => {
    const signal = signalOf(args[arity]);
    if (signal?.aborted === true) return Promise.reject(new RuntimeFailure(interrupt(undefined)));
    const fiber = fork(() => withOwner(() => args.slice(0, arity).reduce(apply, fn), "facade"), currentCtx());
    const stop = (): void => fiber.interrupt(-1);
    signal?.addEventListener("abort", stop, { once: true });
    return new Promise((resolve, reject) => {
      fiber.observe((exit) => {
        signal?.removeEventListener("abort", stop);
        if (exit.cause === undefined) return resolve(ok(exit.value));
        const fail = recoverable(exit.cause);
        return fail ? resolve(err(fail.error)) : reject(new RuntimeFailure(exit.cause));
      });
    });
  };
