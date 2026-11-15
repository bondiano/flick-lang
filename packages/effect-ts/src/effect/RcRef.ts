// Effect's `RcRef` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes. Acquisition
// runs under a lease holder so Phase M `Within` cannot park release on an
// outer lexical owner.
import { type Duration, Effect, Option, RcRef, type Scope } from "effect";

import { acquireHolder, retire } from "./lease.ts";

export * from "effect/RcRef";

/** `RcRef.make`: the options record with `Option` fields. */
export const flickMake = <A, E, R>(options: {
  readonly acquire: Effect.Effect<A, E, R>;
  readonly idleTimeToLive: Option.Option<Duration.Duration>;
}): Effect.Effect<RcRef.RcRef<A, E>, never, Exclude<R, Scope.Scope> | Scope.Scope> =>
  RcRef.make({
    acquire: Effect.uninterruptibleMask((restore) =>
      Effect.gen(function* () {
        const holder = yield* acquireHolder(options.acquire, restore);
        yield* Effect.addFinalizer(() => retire(holder));
        return holder.value;
      }),
    ),
    idleTimeToLive: Option.getOrUndefined(options.idleTimeToLive),
  });
