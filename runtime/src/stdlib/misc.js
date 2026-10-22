// Redacted values and event policies.
import { Redacted } from "effect";

export const redactedMake = (value) => Redacted.make(value);
export const redactedReveal = (redacted) => Redacted.value(redacted);

/** `Events.queue capacity`: a bounded FIFO policy. */
export const eventsQueue = (capacity) => {
  if (!Number.isInteger(capacity) || capacity <= 0) {
    throw new RangeError(`Events.queue needs a positive integer capacity, not ${String(capacity)}`);
  }
  return Object.freeze({ capacity });
};
