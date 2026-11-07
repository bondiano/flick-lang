// The Effect side of the crossing measurements.
import { Effect } from "@flick-lang/runtime";
export const now = () => performance.now();
export const eSucceed = (x) => Effect.succeed(x);
export const eSync = (x) => Effect.sync(() => x);
export const eTwoSteps = (x) => Effect.flatMap(Effect.sync(() => x), (y) => Effect.succeed(y));
export const eYield = (x) => Effect.as(Effect.yieldNow, x);
/** An Effect that calls `f` n times, one after the other: each call a crossing on the kernel. */
export const drive = (f, n) =>
  Effect.gen(function* () {
    let acc = 0;
    for (let i = 0; i < n; i++) acc += yield* f(i);
    return acc;
  });
/** The same loop with a callback of Effect's own, nothing crossing. */
export const driveNative = (n) =>
  Effect.gen(function* () {
    let acc = 0;
    const f = (i) => Effect.succeed(i + 1);
    for (let i = 0; i < n; i++) acc += yield* f(i);
    return acc;
  });
