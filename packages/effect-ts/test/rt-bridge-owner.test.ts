import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Exit, Scope } from "effect";

import { close, Owner } from "../src/owner.ts";
import { runIn } from "@flick-lang/runtime/rt/fiber";
import { scopeRun } from "@flick-lang/runtime/rt/owner";
import { callEffect, fromEffect } from "../src/bridge.ts";

test("a synchronous Effect callback can close an empty native scope", () => {
  assert.equal(runIn(fromEffect(Effect.withLogSpan("callback")(
    callEffect(() => scopeRun(() => 42)),
  ))), 42);
});

test("closing an owner without pending work completes synchronously", () => {
  const owner = new Owner(Scope.makeUnsafe());
  let finalized = false;
  Effect.runSync(Scope.addFinalizer(owner.resources, Effect.sync(() => { finalized = true; })));
  const exit = Exit.succeed(42);
  assert.equal(Effect.runSync(close(owner, exit)), exit);
  assert.equal(finalized, true);
  assert.equal(owner.open, false);
});

test("closing an owner waits for pending foreign work before finalizing", async () => {
  const owner = new Owner(Scope.makeUnsafe());
  const events: string[] = [];
  let release: (() => void) | undefined;
  owner.track(new Promise<void>((resolve) => { release = resolve; }));
  Effect.runSync(Scope.addFinalizer(owner.resources, Effect.sync(() => { events.push("finalize"); })));
  const closing = Effect.runPromise(close(owner, Exit.void)).then(() => { events.push("closed"); });
  await Promise.resolve();
  assert.equal(events.length, 0);
  assert.ok(release);
  events.push("settled");
  release();
  await closing;
  assert.deepEqual(events, ["settled", "finalize", "closed"]);
});
