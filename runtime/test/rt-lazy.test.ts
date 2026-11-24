/* oxlint-disable typescript/no-unsafe-type-assertion -- an operation is a function of the program */
// `Lazy` on the kernel: single flight (R9) over fibers.
import assert from "node:assert/strict";
import { test } from "node:test";

import { type Exit, type OpInstall, type Run, currentCtx, die, fail, fork, handle, hasInterrupts, operation, raise, service } from "../src/rt/kernel.ts";
import { lazyForce, lazyMake, lazyMakeLocal } from "../src/rt/effect/Lazy.ts";
import { root, sleep } from "./rt-support.ts";

/** A fiber of `task` and how it ends. */
const start = (task: () => unknown) => {
  const fiber = fork(task, currentCtx());
  return { fiber, exit: new Promise<Exit>((resolve) => fiber.observe(resolve)) };
};

const later = (millis: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, millis));

/** The message of the defect an exit died with. */
const message = (exit: Exit): unknown => {
  const reason = exit.cause?.reasons[0];
  return reason?._tag === "Die" && reason.defect instanceof Error ? reason.defect.message : reason;
};

/** A cell whose initializer counts its runs, waits `millis` and then ends as `end` says. */
const counted = (millis: number, end: (run: number) => unknown = () => 42) => {
  const state = { runs: 0 };
  const cell = lazyMake(function* (): Run {
    const run = ++state.runs;
    yield sleep(millis);
    return end(run);
  });
  return { state, cell };
};

test("a second force gets the cached value without running the initializer again", async () => {
  const { state, cell } = counted(0);
  assert.equal((await root(() => lazyForce(cell))).value, 42);
  assert.equal((await root(() => lazyForce(cell))).value, 42);
  assert.equal(state.runs, 1);
});

test("a typed failure is cached and raised again", async () => {
  const error = { message: "failed" };
  const { state, cell } = counted(0, () => raise(fail(error)));
  assert.deepEqual((await root(() => lazyForce(cell))).cause, fail(error));
  assert.deepEqual((await root(() => lazyForce(cell))).cause, fail(error));
  assert.equal(state.runs, 1);
});

test("a defect is cached and raised again", async () => {
  const { state, cell } = counted(0, () => raise(die("boom")));
  assert.deepEqual((await root(() => lazyForce(cell))).cause, die("boom"));
  assert.deepEqual((await root(() => lazyForce(cell))).cause, die("boom"));
  assert.equal(state.runs, 1);
});

test("concurrent forces run the initializer once and all get its value", async () => {
  const { state, cell } = counted(5);
  const exits = await Promise.all([1, 2, 3].map(() => root(() => lazyForce(cell))));
  assert.deepEqual(exits.map((e) => e.value), [42, 42, 42]);
  assert.equal(state.runs, 1);
});

test("an interrupted owner resets the cell: a waiter runs it again and gets the value", async () => {
  const { state, cell } = counted(20, (run) => run);
  const owner = start(() => lazyForce(cell));
  const waiter = start(() => lazyForce(cell));
  await later(5);
  owner.fiber.interrupt(99);
  const [ownerExit, waiterExit] = await Promise.all([owner.exit, waiter.exit]);
  assert.ok(ownerExit.cause !== undefined && hasInterrupts(ownerExit.cause));
  assert.equal(waiterExit.cause, undefined);
  assert.equal(waiterExit.value, 2);
  assert.equal(state.runs, 2);
  assert.equal((await root(() => lazyForce(cell))).value, 2);
  assert.equal(state.runs, 2);
});

test("an interrupted waiter leaves the attempt running for the others", async () => {
  const { state, cell } = counted(20);
  const owner = start(() => lazyForce(cell));
  const waiter = start(() => lazyForce(cell));
  await later(5);
  waiter.fiber.interrupt(99);
  const [ownerExit, waiterExit] = await Promise.all([owner.exit, waiter.exit]);
  assert.ok(waiterExit.cause !== undefined && hasInterrupts(waiterExit.cause));
  assert.equal(ownerExit.value, 42);
  assert.equal((await root(() => lazyForce(cell))).value, 42);
  assert.equal(state.runs, 1);
  assert.equal(cell.waiters.size, 0);
});

test("forcing a lazy from its own initializer is a defect, and it is cached", async () => {
  let runs = 0;
  const cell = lazyMake(function* (): Run {
    runs++;
    return yield lazyForce(cell);
  });
  assert.equal(message(await root(() => lazyForce(cell))), "lazy forced during its own initialization");
  assert.equal(message(await root(() => lazyForce(cell))), "lazy forced during its own initialization");
  assert.equal(runs, 1);
});

test("the initializer runs with the handlers of the fiber that forces it", async () => {
  const Ask = service("Test.LazyAsk");
  const ask = operation(Ask, "ask", 1) as (u: undefined) => unknown;
  const arms: ReadonlyArray<OpInstall> = [["ask", () => "forcer", "tail", 0, 1]];
  const cell = lazyMake(() => ask(undefined));
  const exit = await root(() => handle([[Ask, arms]], () => lazyForce(cell), undefined, undefined));
  assert.equal(exit.value, "forcer");
});

test("lazyMakeLocal caches values, typed failures, and defects and releases task on Pending -> Running", async () => {
  let runs = 0;
  let taskDuringRun: unknown = "unset";
  const okCell = lazyMakeLocal(function* (): Run {
    runs++;
    taskDuringRun = okCell.task;
    yield sleep(0);
    return 42;
  });
  assert.notEqual(okCell.task, undefined);
  assert.equal((await root(() => lazyForce(okCell))).value, 42);
  assert.equal(taskDuringRun, undefined);
  assert.equal(okCell.task, undefined);
  assert.equal((await root(() => lazyForce(okCell))).value, 42);
  assert.equal(runs, 1);

  let failRuns = 0;
  const error = { message: "local-failed" };
  const failCell = lazyMakeLocal(function* (): Run {
    failRuns++;
    yield sleep(0);
    return raise(fail(error));
  });
  assert.deepEqual((await root(() => lazyForce(failCell))).cause, fail(error));
  assert.deepEqual((await root(() => lazyForce(failCell))).cause, fail(error));
  assert.equal(failRuns, 1);
  assert.equal(failCell.task, undefined);

  let dieRuns = 0;
  const dieCell = lazyMakeLocal(function* (): Run {
    dieRuns++;
    yield sleep(0);
    return raise(die("local-boom"));
  });
  assert.deepEqual((await root(() => lazyForce(dieCell))).cause, die("local-boom"));
  assert.deepEqual((await root(() => lazyForce(dieCell))).cause, die("local-boom"));
  assert.equal(dieRuns, 1);
  assert.equal(dieCell.task, undefined);
});

test("lazyMakeLocal self-force is a cached defect (R9(e))", async () => {
  let runs = 0;
  const cell = lazyMakeLocal(function* (): Run {
    runs++;
    return yield lazyForce(cell);
  });
  assert.equal(message(await root(() => lazyForce(cell))), "lazy forced during its own initialization");
  assert.equal(message(await root(() => lazyForce(cell))), "lazy forced during its own initialization");
  assert.equal(runs, 1);
  assert.equal(cell.task, undefined);
});

test("lazyMakeLocal resets to Pending on interruption and on handler abort", async () => {
  let intRuns = 0;
  const intCell = lazyMakeLocal(function* (): Run {
    const run = ++intRuns;
    yield sleep(20);
    return run;
  });
  const owner = start(() => lazyForce(intCell));
  await later(5);
  owner.fiber.interrupt(99);
  const ownerExit = await owner.exit;
  assert.ok(ownerExit.cause !== undefined && hasInterrupts(ownerExit.cause));
  assert.notEqual(intCell.task, undefined);
  assert.equal((await root(() => lazyForce(intCell))).value, 2);
  assert.equal((await root(() => lazyForce(intCell))).value, 2);
  assert.equal(intRuns, 2);
  assert.equal(intCell.task, undefined);

  const Stop = service("Test.LocalLazyStop");
  const stop = operation(Stop, "stop", 1) as (s: string) => unknown;
  const stopArms: ReadonlyArray<OpInstall> = [["stop", (s: unknown) => s, "abort", 0, 1]];
  let abortRuns = 0;
  const abortCell = lazyMakeLocal(function* (): Run {
    const run = ++abortRuns;
    if (run === 1) return yield stop("stopped");
    return 77;
  });
  const first = await root(() => handle([[Stop, stopArms]], () => lazyForce(abortCell), undefined, undefined));
  assert.equal(first.value, "stopped");
  assert.notEqual(abortCell.task, undefined);
  const second = await root(() => handle([[Stop, stopArms]], () => lazyForce(abortCell), undefined, undefined));
  assert.equal(second.value, 77);
  assert.equal((await root(() => lazyForce(abortCell))).value, 77);
  assert.equal(abortRuns, 2);
  assert.equal(abortCell.task, undefined);
});
