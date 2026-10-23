// Redacted values and event policies.
import { Redacted } from "effect";

import type { QueuePolicy } from "./callbacks.ts";

export const redactedMake = <A>(value: A): Redacted.Redacted<A> => Redacted.make(value);
export const redactedReveal = <A>(redacted: Redacted.Redacted<A>): A => Redacted.value(redacted);

/** `Events.queue capacity`: a bounded FIFO policy. */
export const eventsQueue = (capacity: number): QueuePolicy => {
  if (!Number.isInteger(capacity) || capacity <= 0) {
    throw new RangeError(`Events.queue needs a positive integer capacity, not ${String(capacity)}`);
  }
  return Object.freeze({ capacity });
};
