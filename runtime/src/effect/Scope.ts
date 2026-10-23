// Effect's `Scope` for the bindings of the standard library: finalizers
// added to the scope of the owner a call runs in.
import { Effect, Exit, Option, Scope } from "effect";

import { RawCause } from "../cause.ts";

export * from "effect/Scope";

/** `Scope.add_finalizer`: into the scope of the call. */
export const flickAddFinalizer = <R>(finalizer: Effect.Effect<void, never, R>): Effect.Effect<void, never, R | Scope.Scope> =>
  Effect.flatMap(Effect.context<R>(), (context) =>
    Effect.flatMap(Effect.scope, (scope) => Scope.addFinalizer(scope, Effect.provideContext(finalizer, context))),
  );

/** `Scope.add_finalizer_exit`: `finalizer` sees the cause of a failure. */
export const flickAddFinalizerExit = <R>(
  finalizer: (cause: Option.Option<RawCause>) => Effect.Effect<void, never, R>,
): Effect.Effect<void, never, R | Scope.Scope> =>
  Effect.flatMap(Effect.context<R>(), (context) =>
    Effect.flatMap(Effect.scope, (scope) =>
      Scope.addFinalizerExit(scope, (exit) =>
        Effect.provideContext(
          finalizer(Exit.isSuccess(exit) ? Option.none() : Option.some(new RawCause(exit.cause))),
          context,
        ),
      ),
    ),
  );
