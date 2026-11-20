// Handlers of effects: `try … with` with arms for operations, and
// `provide S impl`. Each effect is a service whose implementation is the
// arms, as `S.Impl` is: a `tail` arm gives the operation its value, run
// with the services of the place of the handler; an `abort` arm unwinds the
// body to the handler and gives it its value. Neither is a typed failure of
// the code in between: a `try` there does not see them (`recoverable`).
import { Cause, Context, Effect, type Fiber } from "effect";

import { OwnerEvidenceContext, readOwnerEvidence } from "./owner_evidence.ts";
import { HandlerSignal, hasRealDie, hasSignal, isOwnHandlerSignal, stripSignals, tryWith } from "./cause.ts";

/** Logical identity of an Effect fiber, overridden by the RT bridge so bridged crossings keep one fiber identity. */
export const fiberIdentityHook: { current: (fiber: Fiber.Fiber<unknown, unknown>) => number } = {
  current: (fiber) => fiber.id,
};

/** The signal of the handler `token` in `cause`. */
const signalOf = (cause: Cause.Cause<unknown>, token: object): HandlerSignal | undefined => {
  for (const reason of cause.reasons) {
    if (Cause.isFailReason(reason) && isOwnHandlerSignal(reason.error) && reason.error.token === token) {
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
  errors: ((error: unknown, unhandled: () => Effect.Effect<never, unknown>) => Effect.Effect<unknown, unknown, unknown> | undefined) | undefined,
): Effect.Effect<unknown, unknown, unknown> =>
  Effect.withFiber((ownerFiber) => {
    const ownerId = fiberIdentityHook.current(ownerFiber);
    return Effect.contextWith((context: Context.Context<never>) => {
      const token = {};
      let inner: Effect.Effect<Outcome, unknown, unknown> = Effect.map(body, (value): Outcome => ({ value }));
      for (const [tag, arms, aborts] of installs) {
        const impl = Object.fromEntries<unknown>(Object.entries(arms).map(([name, arm]) => [
          name,
          aborts.includes(name)
            ? around(arm, (run) =>
                Effect.withFiber((fiber) => {
                  if (fiberIdentityHook.current(fiber) !== ownerId) {
                    return Effect.die(new Error("an `abort` operation ran in a fiber other than its handler's"));
                  }
                  return Effect.flatMap(readOwnerEvidence(), (owner) =>
                    Effect.fail(new HandlerSignal(token, Effect.provideContext(run, Context.add(context, OwnerEvidenceContext, owner)))),
                  );
                }),
              )
            : around(arm, (run) =>
                Effect.flatMap(readOwnerEvidence(), (owner) =>
                  Effect.mapError(Effect.provideContext(run, Context.add(context, OwnerEvidenceContext, owner)), (error) => new HandlerSignal(token, undefined, error))),
              ),
        ]));
        // Context stores one value per tag. Resolve unmentioned operation names
        // from the nearest outer service; a complete provider is still a barrier.
        // Keep selected functions unchanged: each arm retains its own prompt and
        // outer evidence, rather than re-running it as an arm of this handler.
        const outer = Context.getOrUndefined(context, tag);
        const partial = new Proxy(impl, {
          get: (target, name): unknown => Object.hasOwn(target, name)
            ? Reflect.get(target, name)
            : outer === undefined ? undefined : Reflect.get(outer, name),
        });
        inner = Effect.provideService(inner, tag, partial);
      }
      if (errors !== undefined) {
        inner = tryWith(inner, (error, unhandled) => {
          const handled = errors(error, unhandled);
          return handled === undefined ? undefined : Effect.map(handled, (result): Outcome => ({ result }));
        });
      }
      return Effect.matchCauseEffect(inner, {
        onSuccess: (outcome) => {
          if ("result" in outcome) return Effect.succeed(outcome.result);
          return ret === undefined ? Effect.succeed(outcome.value) : ret(outcome.value);
        },
        onFailure: (cause) => {
          if (hasRealDie(cause)) {
            return Effect.failCause(hasSignal(cause) ? stripSignals(cause) : cause);
          }
          const signal = signalOf(cause, token);
          if (signal === undefined) return Effect.failCause(cause);
          return signal.arm ?? Effect.fail(signal.error);
        },
      });
    });
  });
