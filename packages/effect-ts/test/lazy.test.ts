// `Lazy` on Effect: single flight (R9) over fibers.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Context, Deferred, Effect, Exit, Fiber } from "effect";

import { lazyForce, lazyMake, lazyMakeLocal } from "../src/index.ts";
import type { LazyCell, LocalLazyCell } from "../src/effect/Lazy.ts";
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
const twice = async <A, E>(cell: LazyCell<A, E, never> | LocalLazyCell<A, E, never>) =>
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
  const started = Deferred.makeUnsafe<void>();
  let runs = 0;
  const cell = Effect.runSync(
    lazyMake(
      Effect.suspend(() => {
        const run = ++runs;
        if (run === 1) {
          Deferred.doneUnsafe(started, Exit.void);
          return Effect.never;
        }
        return Effect.succeed(run);
      }),
    ),
  );
  const owner = Effect.runFork(lazyForce(cell));
  await Effect.runPromise(Deferred.await(started));
  const waiter = Effect.runFork(lazyForce(cell));
  await later(5);
  const ownerExit = await Effect.runPromise(Effect.andThen(Fiber.interrupt(owner), Fiber.await(owner)));
  const waiterExit = await Effect.runPromise(Fiber.await(waiter));
  assert.ok(Exit.isFailure(ownerExit) && Cause.hasInterruptsOnly(ownerExit.cause));
  assert.deepEqual(waiterExit, Exit.succeed(2));
  assert.equal(runs, 2);
  assert.equal(await Effect.runPromise(lazyForce(cell)), 2);
  assert.equal(runs, 2);
});

test("an interrupted waiter leaves the attempt running for the others", async () => {
  const started = Deferred.makeUnsafe<void>();
  const release = Deferred.makeUnsafe<void>();
  let runs = 0;
  const cell = Effect.runSync(
    lazyMake(
      Effect.suspend(() => {
        runs++;
        Deferred.doneUnsafe(started, Exit.void);
        return Effect.andThen(Deferred.await(release), Effect.succeed(42));
      }),
    ),
  );
  const owner = Effect.runFork(lazyForce(cell));
  await Effect.runPromise(Deferred.await(started));
  const waiter = Effect.runFork(lazyForce(cell));
  await later(5);
  const waiterExit = await Effect.runPromise(Effect.andThen(Fiber.interrupt(waiter), Fiber.await(waiter)));
  assert.ok(Exit.isFailure(waiterExit) && Cause.hasInterruptsOnly(waiterExit.cause));
  Deferred.doneUnsafe(release, Exit.void);
  assert.deepEqual(await Effect.runPromise(Fiber.await(owner)), Exit.succeed(42));
  assert.equal(await Effect.runPromise(lazyForce(cell)), 42);
  assert.equal(runs, 1);
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

test("lazyMakeLocal caches values, failures, and defects and releases task on Pending -> Running", async () => {
  let runs = 0;
  let taskDuringRun: unknown = "unset";
  const okCell: LocalLazyCell<number, never, never> = Effect.runSync(
    lazyMakeLocal(
      Effect.suspend(() => {
        runs++;
        taskDuringRun = okCell.task;
        return Effect.succeed(42);
      }),
    ),
  );
  assert.notEqual(okCell.task, undefined);
  assert.equal(await Effect.runPromise(lazyForce(okCell)), 42);
  assert.equal(taskDuringRun, undefined);
  assert.equal(okCell.task, undefined);
  assert.equal(await Effect.runPromise(lazyForce(okCell)), 42);
  assert.equal(runs, 1);

  let failRuns = 0;
  const failCell = Effect.runSync(
    lazyMakeLocal(
      Effect.suspend(() => {
        failRuns++;
        return Effect.fail("local-failed");
      }),
    ),
  );
  assert.deepEqual(errorsOf(await Effect.runPromiseExit(lazyForce(failCell))), ["local-failed"]);
  assert.deepEqual(errorsOf(await Effect.runPromiseExit(lazyForce(failCell))), ["local-failed"]);
  assert.equal(failRuns, 1);
  assert.equal(failCell.task, undefined);

  let dieRuns = 0;
  const dieCell = Effect.runSync(
    lazyMakeLocal(
      Effect.suspend(() => {
        dieRuns++;
        return Effect.die("local-boom");
      }),
    ),
  );
  for (const cause of await twice(dieCell)) {
    assert.deepEqual(cause.reasons.flatMap((r) => (Cause.isDieReason(r) ? [r.defect] : [])), ["local-boom"]);
  }
  assert.equal(dieRuns, 1);
  assert.equal(dieCell.task, undefined);
});

test("lazyMakeLocal self-force is a cached defect (R9(e))", async () => {
  let runs = 0;
  const cell: LocalLazyCell<unknown, never, never> = Effect.runSync(
    lazyMakeLocal(
      Effect.suspend((): Effect.Effect<unknown> => {
        runs++;
        return lazyForce(cell);
      }),
    ),
  );
  for (const cause of await twice(cell)) {
    const defects = cause.reasons.flatMap((r) => (Cause.isDieReason(r) && r.defect instanceof Error ? [r.defect.message] : []));
    assert.deepEqual(defects, ["lazy forced during its own initialization"]);
  }
  assert.equal(runs, 1);
  assert.equal(cell.task, undefined);
});

test("lazyMakeLocal resets on interruption and runs with forcing fiber services", async () => {
  const started = Deferred.makeUnsafe<void>();
  let runs = 0;
  const cell = Effect.runSync(
    lazyMakeLocal(
      Effect.suspend(() => {
        const run = ++runs;
        if (run === 1) {
          Deferred.doneUnsafe(started, Exit.void);
          return Effect.never;
        }
        return Effect.succeed(run);
      }),
    ),
  );
  const owner = Effect.runFork(lazyForce(cell));
  await Effect.runPromise(Deferred.await(started));
  const ownerExit = await Effect.runPromise(Effect.andThen(Fiber.interrupt(owner), Fiber.await(owner)));
  assert.ok(Exit.isFailure(ownerExit) && Cause.hasInterruptsOnly(ownerExit.cause));
  assert.notEqual(cell.task, undefined);
  assert.equal(await Effect.runPromise(lazyForce(cell)), 2);
  assert.equal(await Effect.runPromise(lazyForce(cell)), 2);
  assert.equal(runs, 2);
  assert.equal(cell.task, undefined);

  const Name = Context.Service<string>("Test.LocalLazyName");
  const svcCell = Effect.runSync(lazyMakeLocal(Effect.service(Name)));
  assert.equal(await Effect.runPromise(Effect.provideService(lazyForce(svcCell), Name, "local-forcer")), "local-forcer");
});
