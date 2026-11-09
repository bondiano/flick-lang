// Effect's `Deferred` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Deferred, Effect } from "effect";

import type { Failure, FlickExit } from "../cause.ts";
import { failureCause, nativeExit } from "../stdlib/outcome.ts";

export * from "effect/Deferred";

/** `Deferred.done`: completes with a Flick `Exit`. */
export const flickDone =
  <A>(exit: FlickExit<A>) =>
  <E>(self: Deferred.Deferred<A, E>): Effect.Effect<boolean> =>
    Effect.suspend(() => Deferred.done(self, nativeExit<A, E>(exit)));

/** `Deferred.fail_cause`: fails with the cause behind a Flick `Failure`. */
export const flickFailCause =
  (failure: Failure) =>
  <A, E>(self: Deferred.Deferred<A, E>): Effect.Effect<boolean> =>
    Effect.suspend(() => Deferred.failCause(self, failureCause<E>(failure)));

/** `Deferred.fail_cause_sync`: `f ()` gives the Flick `Failure`. */
export const flickFailCauseSync =
  (f: () => Failure) =>
  <A, E>(self: Deferred.Deferred<A, E>): Effect.Effect<boolean> =>
    Effect.suspend(() => Deferred.failCause(self, failureCause<E>(f())));
