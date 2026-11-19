import assert from "node:assert/strict";
import { test } from "node:test";

import { Context, Effect, Fiber, Layer } from "effect";

import { flickFromAcquire, flickSet } from "../src/effect/ScopedRef.ts";
import { acquireRelease, scope } from "../src/index.ts";
import { readOwnerEvidence, within } from "../src/owner.ts";
import { acquireHolder, retire } from "../src/effect/lease.ts";
import * as FlickLayer from "../src/effect/Layer.ts";

test("an effect layer supplies its build owner and releases before the caller scope", async () => {
  const released: Array<string> = [];
  await Effect.runPromise(scope((outer) => Effect.gen(function* () {
    const layer = FlickLayer.effectContext(Effect.gen(function* () {
      const owner = yield* readOwnerEvidence();
      assert.notEqual(owner, outer);
      yield* within(owner, acquireRelease(Effect.void, () => Effect.sync(() => released.push("layer"))));
      return Context.empty();
    }));
    yield* Effect.scoped(Layer.build(layer));
    assert.deepEqual(released, ["layer"]);
  })));
});

test("a library acquisition supplies its own owner evidence and releases on retirement", async () => {
  const released: Array<string> = [];
  await Effect.runPromise(scope((outer) => Effect.gen(function* () {
    const holder = yield* acquireHolder(Effect.gen(function* () {
      const owner = yield* readOwnerEvidence();
      assert.notEqual(owner, outer);
      return yield* within(owner, acquireRelease(Effect.succeed("item"), () => Effect.sync(() => released.push("item"))));
    }));
    yield* retire(holder);
    assert.deepEqual(released, ["item"]);
    assert.equal(yield* readOwnerEvidence(), outer);
  })));
  assert.deepEqual(released, ["item"]);
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
