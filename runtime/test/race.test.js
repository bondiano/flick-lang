import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Exit } from "effect";

import { raceFirst, raceSuccess, timeout, timeoutOption } from "../src/index.js";

const loser = (log, name) =>
  Effect.onInterrupt(Effect.never, () =>
    Effect.andThen(Effect.sleep(10), Effect.sync(() => log.push(`${name} cleaned up`))),
  );

test("race_first ends with the first branch and waits for the loser", async () => {
  const log = [];
  const exit = await Effect.runPromiseExit(
    raceFirst(Effect.andThen(Effect.sleep(1), Effect.fail("first")), loser(log, "b")),
  );
  log.push("returned");
  assert.deepEqual(exit.cause.reasons.map((r) => r.error), ["first"]);
  assert.deepEqual(log, ["b cleaned up", "returned"]);
});

test("race_success takes the first success and does not add loser failures", async () => {
  const log = [];
  const result = await Effect.runPromise(
    raceSuccess(Effect.fail("early failure"), Effect.andThen(Effect.sleep(5), Effect.succeed("ok"))),
  );
  assert.equal(result, "ok");
  // a winner that is ready at once may win before the loser starts at all
  const fast = Effect.andThen(Effect.sleep(5), Effect.succeed("fast"));
  const won = await Effect.runPromise(raceSuccess(fast, loser(log, "slow")));
  assert.equal(won, "fast");
  assert.deepEqual(log, ["slow cleaned up"]);
});

test("timeout fails with TimeoutError after interrupting and awaiting the work", async () => {
  const log = [];
  const exit = await Effect.runPromiseExit(timeout(5, loser(log, "work")));
  log.push("returned");
  assert.ok(Exit.isFailure(exit));
  assert.deepEqual(exit.cause.reasons.map((r) => r.error), [
    { _tag: "Prelude.TimeoutError.TimeoutError" },
  ]);
  assert.deepEqual(log, ["work cleaned up", "returned"]);
});

test("timeout_option is None on time out and keeps other failures", async () => {
  const none = await Effect.runPromise(timeoutOption(5, Effect.never));
  assert.equal(none._tag, "None");
  const some = await Effect.runPromise(timeoutOption(50, Effect.succeed(1)));
  assert.equal(some.value, 1);
  const failed = await Effect.runPromiseExit(timeoutOption(50, Effect.fail("E")));
  assert.deepEqual(failed.cause.reasons.map((r) => r.error), ["E"]);
});
