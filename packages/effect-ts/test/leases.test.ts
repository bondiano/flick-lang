import assert from "node:assert/strict";
import { test } from "node:test";

import { Deferred, Effect, Fiber } from "effect";

import { flickFromAcquire, flickSet, flickUse } from "../src/effect/ScopedRef.ts";
import { acquireRelease, readOwnerEvidence, scope } from "../src/index.ts";
import { acquireHolder, retire } from "../src/effect/lease.ts";
import { CurrentOwner, type FlickFiber, fiberSpawn, within } from "../src/owner.ts";

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

test("a library acquisition respects an explicit outer owner activation", async () => {
  const selected = await Effect.runPromise(scope(() => Effect.gen(function* () {
    const outer = yield* Effect.service(CurrentOwner);
    assert.ok(outer);
    const holder = yield* acquireHolder(within(outer, Effect.service(CurrentOwner)));
    const activated = holder.value === outer;
    yield* retire(holder);
    return activated;
  })));
  assert.equal(selected, true);
});

test("retiring an acquisition drains its owned children before releasing resources", async () => {
  const events: Array<string> = [];
  const observed = await Effect.runPromise(scope(() => Effect.gen(function* () {
    const entered = yield* Deferred.make<void>();
    let child: FlickFiber<never, never> | undefined;
    const holder = yield* acquireHolder(Effect.gen(function* () {
      const owner = yield* Effect.service(CurrentOwner);
      assert.ok(owner);
      yield* Effect.addFinalizer(() => Effect.sync(() => events.push("resource")));
      child = yield* fiberSpawn(owner, () => Effect.void, Effect.interruptible(Effect.onInterrupt(
        Effect.andThen(Deferred.succeed(entered, undefined), Effect.never),
        () => Effect.sync(() => events.push("child")),
      )));
      yield* Deferred.await(entered);
      return 0;
    }));
    yield* retire(holder);
    const beforeCleanup = [...events];
    if (child !== undefined) yield* Fiber.interrupt(child.fiber);
    return beforeCleanup;
  })));
  assert.deepEqual(observed, ["child", "resource"]);
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
