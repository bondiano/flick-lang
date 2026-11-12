// Redacted values and event policies.
import { Redacted } from "effect";
import { redactedHooks, redactedReveal as nativeReveal, type Redacted as NativeRedacted } from "@flick-lang/runtime/rt/redacted";

import type { QueuePolicy } from "./callbacks.ts";

redactedHooks.reveal = (value) => Redacted.isRedacted(value) ? { value: Redacted.value(value) } : undefined;

export const redactedMake = <A>(value: A): Redacted.Redacted<A> => Redacted.make(value);
export const redactedReveal = <A>(redacted: Redacted.Redacted<A> | NativeRedacted<A>): A =>
  Redacted.isRedacted(redacted) ? Redacted.value(redacted) : nativeReveal(redacted);

/** `Events.queue capacity`: a bounded FIFO policy. */
export const eventsQueue = (capacity: number): QueuePolicy => {
  if (!Number.isInteger(capacity) || capacity <= 0) {
    throw new RangeError(`Events.queue needs a positive integer capacity, not ${String(capacity)}`);
  }
  return Object.freeze({ capacity });
};

export { jsErrorMessage } from "@flick-lang/runtime/rt/describe";
