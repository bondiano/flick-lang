import assert from "node:assert/strict";
import { test } from "node:test";

import { Deferred, Effect } from "effect";

import * as FiberHandle from "../src/effect/FiberHandle.ts";
import * as FiberMap from "../src/effect/FiberMap.ts";
import * as FiberSet from "../src/effect/FiberSet.ts";
import { acquireRelease, scope } from "../src/index.ts";

const options = { propagateInterruption: false, startImmediately: true };

/** How each collection is made and how it runs a task. */
const collections = [
  ["FiberSet", () => Effect.map(FiberSet.flickMake(), (set) => (task: Effect.Effect<void>) => FiberSet.flickRun(options)(task)(set))],
  ["FiberMap", () => Effect.map(FiberMap.flickMake<string, void, never>(), (map) => (task: Effect.Effect<void>) => FiberMap.flickRun({ ...options, onlyIfMissing: false })("k")(task)(map))],
  ["FiberHandle", () => Effect.map(FiberHandle.flickMake(), (handle) => (task: Effect.Effect<void>) => FiberHandle.flickRun({ ...options, onlyIfMissing: false })(task)(handle))],
] as const;

for (const [name, make] of collections) {
  test(`${name}: a closing owner interrupts its children before it releases a later resource`, async () => {
    const log: Array<string> = [];
    let released = false;
    await Effect.runPromise(
      scope(() =>
        Effect.gen(function* () {
          const run = yield* make();
          yield* acquireRelease(Effect.void, () =>
            Effect.sync(() => {
              released = true;
              log.push("release");
            }),
          );
          const started = yield* Deferred.make<void>();
          const child = Effect.andThen(Deferred.succeed(started, undefined), Effect.never).pipe(
            Effect.onInterrupt(() => Effect.sync(() => log.push(`interrupt: released=${released}`))),
          );
          yield* run(child);
          // the barrier: the child runs before the scope closes
          yield* Deferred.await(started);
        }),
      ),
    );
    assert.deepEqual(log, ["interrupt: released=false", "release"]);
  });

  test(`${name}: a child that ended is not interrupted, and a closed owner starts none`, async () => {
    const log: Array<string> = [];
    let later: ((task: Effect.Effect<void>) => Effect.Effect<unknown>) | undefined;
    await Effect.runPromise(
      scope(() =>
        Effect.gen(function* () {
          const run = yield* make();
          later = run;
          yield* acquireRelease(Effect.void, () => Effect.sync(() => log.push("release")));
          const done = yield* Deferred.make<void>();
          yield* run(Effect.andThen(Effect.sync(() => log.push("ran")), Deferred.succeed(done, undefined)));
          yield* Deferred.await(done);
        }),
      ),
    );
    assert.deepEqual(log, ["ran", "release"]);
    // the owner is closed: a task run now never starts
    if (later !== undefined) await Effect.runPromiseExit(later(Effect.sync(() => log.push("late"))));
    await Effect.runPromise(Effect.yieldNow);
    assert.deepEqual(log, ["ran", "release"]);
  });
}
