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

const dataProperty = (value: object, key: string): unknown => {
  const own = Object.getOwnPropertyDescriptor(value, key);
  if (own !== undefined) return "value" in own ? own.value : undefined;
  const inherited: unknown = Object.getPrototypeOf(value);
  return typeof inherited === "object" && inherited !== null ? dataProperty(inherited, key) : undefined;
};

/** `JsError.from_js`: what a thrown value says, without running its getters. */
export const jsErrorMessage = (value: unknown): string => {
  if (typeof value === "string") return value;
  try {
    if (value instanceof Error) {
      const name = dataProperty(value, "name");
      const message = dataProperty(value, "message");
      return `${typeof name === "string" ? name : "Error"}: ${typeof message === "string" ? message : ""}`;
    }
    return Object.prototype.toString.call(value);
  } catch {
    // a revoked or throwing proxy
    return "[object Unknown]";
  }
};
