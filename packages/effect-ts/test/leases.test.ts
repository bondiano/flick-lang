import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Fiber } from "effect";

import { flickFromAcquire, flickSet, flickUse } from "../src/effect/ScopedRef.ts";
import { acquireRelease, readOwnerEvidence, scope, within } from "../src/index.ts";

test("a polymorphic holder acquisition uses the holder's evidence until its last lease ends", async () => {
  const released: string[] = [];
  const resource = (name: string) => Effect.flatMap(readOwnerEvidence(), (owner) =>
    within(owner, acquireRelease(Effect.succeed(name), () => Effect.sync(() => { released.push(name); }))));
  await Effect.runPromise(scope(() => Effect.gen(function* () {
    const ref = yield* flickFromAcquire(resource("first"));
    yield* flickUse(ref, () => Effect.gen(function* () {
      yield* flickSet(resource("second"))(ref);
      assert.deepEqual(released, []);
    }));
    assert.deepEqual(released, ["first"]);
  })));
  assert.deepEqual(released, ["first", "second"]);
});

test("an interrupted ScopedRef.set releases what it acquired and keeps the previous resource, each released once", async () => {
  const released: Array<string> = [];
  const resource = (name: string) =>
    acquireRelease(Effect.succeed(name), () => Effect.sync(() => released.push(name)));
  await Effect.runPromise(
    scope(() =>
      Effect.gen(function* () {
        const ref = yield* flickFromAcquire(resource("first"));
        // the setter is interrupted as soon as the replacement is acquired, before it is swapped in
        const setter = yield* Effect.forkChild(
          Effect.withFiber((fiber) =>
            flickSet(Effect.tap(resource("second"), () => Effect.sync(() => fiber.interruptUnsafe())))(ref),
          ),
        );
        yield* Fiber.await(setter);
      }),
    ),
  );
  assert.deepEqual(released, ["second", "first"]);
});
