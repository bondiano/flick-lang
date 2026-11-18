// The bindings of `Clock` on the kernel: the clock of the fiber, read anew by each call.
import { clock } from "../clock.ts";

export const flickCurrentTimeMillis = (): number => clock().millis();
export const flickCurrentTimeNanos = (): bigint => clock().nanos();
export const flickMonotonicTimeNanos = (): bigint => clock().monotonic();
