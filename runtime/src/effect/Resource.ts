// `Resource`: a value `manual` acquires and `refresh` acquires again. Its
// value is used only under a lease (`use`); the previous value is released
// once the last lease of it ends, and the last one when the owner of the
// resource's region closes.
import { Effect, Exit, Scope, Semaphore } from "effect";

import { type Holder, acquireHolder, leased, retire } from "./lease.ts";

/** `Resource s e a`: the last successful acquisition, or the first failure. */
export interface FlickResource<A, E> {
  current: Exit.Exit<Holder<A>, E>;
  /** `acquire` with the services of the call that made the resource. */
  readonly again: Effect.Effect<Holder<A>, E>;
  readonly lock: Semaphore.Semaphore;
}

/** `Resource.manual acquire`. */
export const flickManual = <A, E, R>(
  acquire: Effect.Effect<A, E, R>,
): Effect.Effect<FlickResource<A, E>, never, Exclude<R, Scope.Scope> | Scope.Scope> =>
  Effect.gen(function* () {
    const context = yield* Effect.context<Exclude<R, Scope.Scope>>();
    const again = Effect.provideContext(acquireHolder(acquire), context);
    const resource: FlickResource<A, E> = {
      current: yield* Effect.exit(again),
      again,
      lock: Semaphore.makeUnsafe(1),
    };
    yield* Scope.addFinalizer(
      yield* Scope.Scope,
      Effect.suspend(() => (Exit.isSuccess(resource.current) ? retire(resource.current.value) : Effect.void)),
    );
    return resource;
  });

/** `Resource.use resource f`: fails as the acquisition did if it never succeeded. */
export const flickUse = <A, B, E, E2, R>(
  resource: FlickResource<A, E>,
  f: (value: A) => Effect.Effect<B, E2, R>,
): Effect.Effect<B, E | E2, R> =>
  leased(
    Effect.suspend(() => {
      const current = resource.current;
      return Exit.isSuccess(current) ? Effect.succeed(current.value) : Effect.failCause(current.cause);
    }),
    f,
  );

/** `Resource.refresh resource`: if the acquisition fails, the previous value stays. */
export const flickRefresh = <A, E>(resource: FlickResource<A, E>): Effect.Effect<void, E> =>
  resource.lock.withPermit(
    Effect.flatMap(Effect.exit(resource.again), (exit) => {
      const previous = resource.current;
      if (Exit.isFailure(exit)) {
        if (Exit.isFailure(previous)) resource.current = exit;
        return Effect.failCause(exit.cause);
      }
      resource.current = exit;
      return Exit.isSuccess(previous) ? retire(previous.value) : Effect.void;
    }),
  );
