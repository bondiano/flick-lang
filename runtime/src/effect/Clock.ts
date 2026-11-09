// The bindings of `Clock` on the kernel: the clock of the fiber, read anew by each call.
import { type Clock, clock } from "../clock.ts";

export const clockWith = (f: (clock: Clock) => unknown): unknown => f(clock());

export const flickMillisOf = (c: Clock): number => c.millis();
export const flickNanosOf = (c: Clock): bigint => c.nanos();
export const flickMonotonicNanosOf = (c: Clock): bigint => c.monotonic();

export const flickCurrentTimeMillis = (): number => clock().millis();
export const flickCurrentTimeNanos = (): bigint => clock().nanos();
export const flickMonotonicTimeNanos = (): bigint => clock().monotonic();
