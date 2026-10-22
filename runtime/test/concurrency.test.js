import assert from "node:assert/strict";
import { test } from "node:test";

import { Cause, Effect, Exit } from "effect";

import {
  exitFailure,
  exitValue,
  failurePrimary,
  parallel,
  parallelAll,
  parallelExits,
  parallelResults,
} from "../src/index.js";

const branch = (log, name, millis, end = Effect.succeed(name)) =>
  Effect.onInterrupt(
    Effect.andThen(Effect.sleep(millis), end),
    () =>
      Effect.andThen(
        Effect.sleep(10),
        Effect.sync(() => log.push(`${name} cleaned up`)),
      ),
  );

test("parallel keeps the order of its input", async () => {
  const log = [];
  const result = await Effect.runPromise(parallel(branch(log, "slow", 20), branch(log, "fast", 1)));
  assert.deepEqual(result, ["slow", "fast"]);
});

test("a failing branch interrupts the others and waits for their cleanup", async () => {
  const log = [];
  const exit = await Effect.runPromiseExit(
    parallelAll(3, [
      branch(log, "a", 50),
      branch(log, "b", 1, Effect.fail({ _tag: "Boom" })),
      branch(log, "c", 50),
    ]),
  );
  log.push("returned");
  assert.ok(Exit.isFailure(exit));
  assert.deepEqual(exit.cause.reasons.filter(Cause.isFailReason).map((r) => r.error), [
    { _tag: "Boom" },
  ]);
  assert.deepEqual(log.sort(), ["a cleaned up", "c cleaned up", "returned"]);
  assert.equal(log.at(-1), "returned");
});

test("parallel_all starts no more than its limit and no new work after a failure", async () => {
  let running = 0;
  let peak = 0;
  const started = [];
  const work = (i) =>
    Effect.suspend(() => {
      started.push(i);
      running += 1;
      peak = Math.max(peak, running);
      return Effect.ensuring(
        i === 2 ? Effect.fail("stop") : Effect.sleep(5),
        Effect.sync(() => (running -= 1)),
      );
    });
  await Effect.runPromiseExit(parallelAll(2, [0, 1, 2, 3, 4, 5].map(work)));
  assert.equal(peak, 2);
  assert.ok(!started.includes(5), `started ${started}`);
});

for (const limit of [0, 1.5, -1, Number.POSITIVE_INFINITY, Number.NaN]) {
  test(`parallel_all rejects the limit ${limit}`, async () => {
    const exit = await Effect.runPromiseExit(parallelAll(limit, [Effect.succeed(1)]));
    assert.ok(Exit.isFailure(exit) && Cause.hasDies(exit.cause));
  });
}

test("parallel_results gives each branch's typed failure as a Result", async () => {
  const results = await Effect.runPromise(
    parallelResults([Effect.succeed(1), Effect.fail({ _tag: "NotFound" }), Effect.succeed(3)]),
  );
  assert.deepEqual(
    results.map((r) => [r._tag, r.success ?? r.failure]),
    [
      ["Success", 1],
      ["Failure", { _tag: "NotFound" }],
      ["Success", 3],
    ],
  );
});

test("parallel_results does not hide a defect", async () => {
  const log = [];
  const exit = await Effect.runPromiseExit(
    parallelResults([branch(log, "a", 50), Effect.die("boom"), Effect.fail({ _tag: "E" })]),
  );
  assert.ok(Exit.isFailure(exit) && Cause.hasDies(exit.cause));
  assert.deepEqual(log, ["a cleaned up"]);
});

test("parallel_exits gives the opaque exit of every branch", async () => {
  const exits = await Effect.runPromise(
    parallelExits([Effect.succeed(1), Effect.fail({ _tag: "E" }), Effect.die("d")]),
  );
  assert.equal(exitValue(exits[0]).value, 1);
  assert.deepEqual(failurePrimary(exitFailure(exits[1]).value).value, { _tag: "E" });
  assert.equal(failurePrimary(exitFailure(exits[2]).value)._tag, "None");
});
