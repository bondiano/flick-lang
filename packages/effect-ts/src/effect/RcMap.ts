// Effect's `RcMap` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: a full map
// fails with the Flick error `RcMap.ExceededCapacity`.
import { Cause, type Duration, Effect, Option, RcMap, type Scope } from "effect";

import type { Tagged } from "../data.ts";
import { tag } from "@flick-lang/runtime/tags";

export * from "effect/RcMap";

/** `RcMap.ExceededCapacity`. */
export type ExceededCapacity = Tagged<string, string>;

const EXCEEDED = /*#__PURE__*/ tag("RcMap.ExceededCapacity.ExceededCapacity");

/** `RcMap.make`: the options record with `Option` fields. */
export const flickMake = <K, A, E, R>(options: {
  readonly lookup: (key: K) => Effect.Effect<A, E, R>;
  readonly idleTimeToLive: Option.Option<(key: K) => Duration.Duration>;
  readonly capacity: Option.Option<number>;
}): Effect.Effect<RcMap.RcMap<K, A, E | Cause.ExceededCapacityError>, never, R | Scope.Scope> =>
  RcMap.make({
    lookup: options.lookup,
    idleTimeToLive: Option.getOrUndefined(options.idleTimeToLive),
    capacity: Option.getOrUndefined(options.capacity) ?? Number.POSITIVE_INFINITY,
  });

/** `RcMap.get`: Effect's `ExceededCapacityError` as the Flick error. */
export const flickGet =
  <K>(key: K) =>
  <A, E>(
    self: RcMap.RcMap<K, A, E | Cause.ExceededCapacityError>,
  ): Effect.Effect<A, Exclude<E, Cause.ExceededCapacityError> | ExceededCapacity, Scope.Scope> =>
    Effect.mapError(RcMap.get(self, key), (error) =>
      Cause.isExceededCapacityError(error)
        ? { _tag: EXCEEDED, _0: error.message }
        : // what is not the capacity error is one of the lookup's own
          // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the union without one member
          (error as Exclude<E, Cause.ExceededCapacityError>),
    );

export const flickKeys = <K, A, E>(self: RcMap.RcMap<K, A, E>): Effect.Effect<Array<K>> =>
  Effect.map(RcMap.keys(self), (keys) => Array.from(keys));
