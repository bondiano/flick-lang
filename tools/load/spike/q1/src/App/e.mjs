// Effects of Effect, for the kernel's fibers to run over the bridge.
import { Effect } from "@flick-lang/runtime";
const log = (s) => Effect.sync(() => process.stdout.write(`  effect: ${s}\n`));
export const later = (ms, value) =>
  Effect.gen(function* () {
    yield* log(`start ${value}`);
    yield* Effect.sleep(ms);
    yield* log(`end ${value}`);
    return value;
  });
export class Boom { constructor(m) { this._tag = "Boom"; this.m = m; } }
export const fails = (e) => Effect.flatMap(Effect.sleep(1), () => Effect.fail(e));
export const dies = (m) => Effect.flatMap(Effect.sleep(1), () => Effect.die(new Error(m)));
export const slowFinalizer = (ms) =>
  Effect.onInterrupt(
    Effect.andThen(log("running, will never end"), Effect.never),
    () => Effect.gen(function* () {
      yield* log("interrupted, finalizer starts");
      yield* Effect.sleep(ms);
      yield* log("finalizer ends");
    }),
  );
export const immediate = (v) => Effect.succeed(v);
export const syncly = (v) => Effect.sync(() => v);
