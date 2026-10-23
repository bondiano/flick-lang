// Effect's `TxQueue` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { type Effect, TxQueue } from "effect";

import { type Failure, nativeCause } from "../cause.ts";

export * from "effect/TxQueue";

/** `TxQueue.fail_cause`: closes the queue with the native cause behind the `Failure`. */
export const flickFailCause = <A>(failure: Failure, self: TxQueue.TxQueue<A, unknown>): Effect.Effect<boolean> => {
  const cause = nativeCause(failure);
  if (cause === undefined) throw new TypeError("not a Failure of this runtime");
  return TxQueue.failCause(self, cause);
};
