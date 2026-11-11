import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Exit, Scope } from "effect";

import { close, closeOwn, CurrentOwner, Owner } from "../src/owner.ts";
import { runIn } from "@flick-lang/runtime/rt/fiber";
import { addFinalizer, scopeRun } from "@flick-lang/runtime/rt/owner";
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

test("a bridged external source stops before native resources and stops only once", () => {
  const events: string[] = [];
  runIn(scopeRun(() => (function* () {
    yield fromEffect(Effect.flatMap(Effect.service(CurrentOwner), (owner) => {
      assert.ok(owner);
      return owner.onClose(() => Effect.sync(() => { events.push("stop source"); }));
    }));
    yield addFinalizer(() => { events.push("release resource"); });
    return undefined;
  })()));
  assert.deepEqual(events, ["stop source", "release resource"]);
});

test("external-scope cleanup observes a failed body without returning its failure again", () => {
  const owner = new Owner(Scope.makeUnsafe());
  const body = Exit.fail("body");
  let observed: Exit.Exit<unknown, unknown> | undefined;
  Effect.runSync(Scope.addFinalizerExit(owner.resources, (exit) => Effect.sync(() => { observed = exit; })));
  assert.ok(Exit.isSuccess(Effect.runSync(closeOwn(owner, body))));
  assert.equal(observed, body);
});

test("external-scope cleanup retains its own defects", () => {
  const owner = new Owner(Scope.makeUnsafe());
  Effect.runSync(Scope.addFinalizer(owner.resources, Effect.die("cleanup")));
  const cleanup = Effect.runSync(closeOwn(owner, Exit.void));
  assert.ok(Exit.isFailure(cleanup));
  assert.equal(cleanup.cause.reasons[0]?._tag, "Die");
});
