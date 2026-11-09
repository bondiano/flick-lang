// Handlers of effects: `try … with` with arms for operations, and
// `provide S impl`. Each effect is a service whose implementation is the
// arms, as `S.Impl` is: a `tail` arm gives the operation its value, run
// with the services of the place of the handler; an `abort` arm unwinds the
// body to the handler and gives it its value. Neither is a typed failure of
// the code in between: a `try` there does not see them (`recoverable`).
import { Cause, type Context, Effect } from "effect";

import { HandlerSignal, isHandlerSignal, tryWith } from "./cause.ts";

/** The signal of the handler `token` in `cause`. */
const signalOf = (cause: Cause.Cause<unknown>, token: object): HandlerSignal | undefined => {
  for (const reason of cause.reasons) {
    if (Cause.isFailReason(reason) && isHandlerSignal(reason.error) && reason.error.token === token) {
      return reason.error;
    }
  }
  return undefined;
};

/**
 * A curried function of the program whose effect, once its arguments are
 * all given, is passed to `k`; the dictionaries of a constrained operation
 * come first, all at once.
 */
const around = (f: unknown, k: (arm: Effect.Effect<unknown, unknown, unknown>) => unknown): unknown => {
  if (typeof f !== "function") return f;
  return (...args: unknown[]): unknown => {
    const result: unknown = Reflect.apply(f, undefined, args);
    return Effect.isEffect(result) ? k(result) : around(result, k);
  };
};

/** One effect a handler installs: its tag, the arms by operation, and the operations whose arms abort. */
export type Install = readonly [Context.Service<object, object>, Readonly<Record<string, unknown>>, ReadonlyArray<string>];

type Outcome = { readonly value: unknown } | { readonly result: unknown };

/**
 * The handler of `installs` around `body`: `ret` makes its value of the
 * body's, and `errors` handles the body's typed errors as the arms of a
 * `try` do; their values are the handler's.
 */
export const handle = (
  installs: ReadonlyArray<Install>,
  body: Effect.Effect<unknown, unknown, unknown>,
  ret: ((value: unknown) => Effect.Effect<unknown, unknown, unknown>) | undefined,
  errors: ((error: unknown) => Effect.Effect<unknown, unknown, unknown> | undefined) | undefined,
): Effect.Effect<unknown, unknown, unknown> =>
  Effect.contextWith((context: Context.Context<never>) => {
    const token = {};
    let inner: Effect.Effect<Outcome, unknown, unknown> = Effect.map(body, (value): Outcome => ({ value }));
    for (const [tag, arms, aborts] of installs) {
      const impl: Record<string, unknown> = {};
      for (const [name, arm] of Object.entries(arms)) {
        impl[name] = aborts.includes(name)
          ? around(arm, (run) => Effect.fail(new HandlerSignal(token, run)))
          : around(arm, (run) =>
              Effect.mapError(Effect.provideContext(run, context), (error) => new HandlerSignal(token, undefined, error)),
            );
      }
      inner = Effect.provideService(inner, tag, impl);
    }
    if (errors !== undefined) {
      inner = tryWith(inner, (error) => {
        const handled = errors(error);
        return handled === undefined ? undefined : Effect.map(handled, (result): Outcome => ({ result }));
      });
    }
    return Effect.matchCauseEffect(inner, {
      onSuccess: (outcome) => {
        if ("result" in outcome) return Effect.succeed(outcome.result);
        return ret === undefined ? Effect.succeed(outcome.value) : ret(outcome.value);
      },
      onFailure: (cause) => {
        const signal = signalOf(cause, token);
        if (signal === undefined) return Effect.failCause(cause);
        return signal.arm ?? Effect.fail(signal.error);
      },
    });
  });
