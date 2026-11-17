// `Lazy` on Effect: single flight (R9) over fibers.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Context, Effect, Exit, Fiber } from "effect";

import { lazyForce, lazyMake } from "../src/index.ts";
import type { LazyCell } from "../src/effect/Lazy.ts";
import { causeOf, errorsOf } from "./support.ts";

/** A cell whose initializer counts its runs, waits `millis` and then ends as `end` says. */
const counted = <A, E>(millis: number, end: (run: number) => Effect.Effect<A, E>) => {
  const state = { runs: 0 };
  const task = Effect.suspend(() => {
    const run = ++state.runs;
    return Effect.andThen(Effect.sleep(millis), end(run));
  });
  return { state, cell: Effect.runSync(lazyMake(task)) };
};

const later = (millis: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, millis));

/** The causes of two forces of `cell`, one after the other. */
const twice = async <A, E>(cell: LazyCell<A, E, never>) =>
  (await Effect.runPromise(Effect.all([Effect.exit(lazyForce(cell)), Effect.exit(lazyForce(cell))]))).map(causeOf);

test("a second force gets the cached value without running the initializer again", async () => {
  const { state, cell } = counted(0, () => Effect.succeed(42));
  assert.equal(await Effect.runPromise(lazyForce(cell)), 42);
  assert.equal(await Effect.runPromise(lazyForce(cell)), 42);
  assert.equal(state.runs, 1);
});

test("a typed failure is cached and raised again", async () => {
  const { state, cell } = counted(0, () => Effect.fail("failed"));
  assert.deepEqual(errorsOf(await Effect.runPromiseExit(lazyForce(cell))), ["failed"]);
  assert.deepEqual(errorsOf(await Effect.runPromiseExit(lazyForce(cell))), ["failed"]);
  assert.equal(state.runs, 1);
});

test("a defect is cached and raised again", async () => {
  const { state, cell } = counted(0, () => Effect.die("boom"));
  for (const cause of await twice(cell)) {
    assert.deepEqual(cause.reasons.flatMap((r) => (Cause.isDieReason(r) ? [r.defect] : [])), ["boom"]);
  }
  assert.equal(state.runs, 1);
});

test("concurrent forces run the initializer once and all get its value", async () => {
  const { state, cell } = counted(5, () => Effect.succeed(42));
  const values = await Effect.runPromise(Effect.all([1, 2, 3].map(() => lazyForce(cell)), { concurrency: "unbounded" }));
  assert.deepEqual(values, [42, 42, 42]);
  assert.equal(state.runs, 1);
});

test("an interrupted owner resets the cell: a waiter runs it again and gets the value", async () => {
  const { state, cell } = counted(20, (run) => Effect.succeed(run));
  const owner = Effect.runFork(lazyForce(cell));
  await later(5);
  const waiter = Effect.runFork(lazyForce(cell));
  await later(5);
  const ownerExit = await Effect.runPromise(Effect.andThen(Fiber.interrupt(owner), Fiber.await(owner)));
  const waiterExit = await Effect.runPromise(Fiber.await(waiter));
  assert.ok(Exit.isFailure(ownerExit) && Cause.hasInterruptsOnly(ownerExit.cause));
  assert.deepEqual(waiterExit, Exit.succeed(2));
  assert.equal(state.runs, 2);
  assert.equal(await Effect.runPromise(lazyForce(cell)), 2);
  assert.equal(state.runs, 2);
});

test("an interrupted waiter leaves the attempt running for the others", async () => {
  const { state, cell } = counted(20, () => Effect.succeed(42));
  const owner = Effect.runFork(lazyForce(cell));
  await later(5);
  const waiter = Effect.runFork(lazyForce(cell));
  await later(5);
  const waiterExit = await Effect.runPromise(Effect.andThen(Fiber.interrupt(waiter), Fiber.await(waiter)));
  assert.ok(Exit.isFailure(waiterExit) && Cause.hasInterruptsOnly(waiterExit.cause));
  assert.deepEqual(await Effect.runPromise(Fiber.await(owner)), Exit.succeed(42));
  assert.equal(await Effect.runPromise(lazyForce(cell)), 42);
  assert.equal(state.runs, 1);
});

test("forcing a lazy from its own initializer is a defect, and it is cached", async () => {
  let runs = 0;
  const cell: LazyCell<unknown, never, never> = Effect.runSync(lazyMake(Effect.suspend((): Effect.Effect<unknown> => {
    runs++;
    return lazyForce(cell);
  })));
  for (const cause of await twice(cell)) {
    const defects = cause.reasons.flatMap((r) => (Cause.isDieReason(r) && r.defect instanceof Error ? [r.defect.message] : []));
    assert.deepEqual(defects, ["lazy forced during its own initialization"]);
  }
  assert.equal(runs, 1);
});

test("the initializer runs with the services of the fiber that forces it", async () => {
  const Name = Context.Service<string>("Test.LazyName");
  const cell = Effect.runSync(lazyMake(Effect.service(Name)));
  assert.equal(await Effect.runPromise(Effect.provideService(lazyForce(cell), Name, "forcer")), "forcer");
});
