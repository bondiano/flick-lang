import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Option } from "effect";

import { raceFirst, raceSuccess, timeout, timeoutOption } from "../src/index.ts";
import { tag } from "../src/tags.ts";
import { errorsOf, value } from "./support.ts";

const loser = (log: Array<string>, name: string) =>
  Effect.onInterrupt(Effect.never, () =>
    Effect.andThen(Effect.sleep(10), Effect.sync(() => log.push(`${name} cleaned up`))),
  );

test("race_first ends with the first branch and waits for the loser", async () => {
  const log: Array<string> = [];
  const exit = await Effect.runPromiseExit(
    raceFirst(Effect.andThen(Effect.sleep(1), Effect.fail("first")), loser(log, "b")),
  );
  log.push("returned");
  assert.deepEqual(errorsOf(exit), ["first"]);
  assert.deepEqual(log, ["b cleaned up", "returned"]);
});

test("race_success takes the first success and does not add loser failures", async () => {
  const log: Array<string> = [];
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
  const log: Array<string> = [];
  const exit = await Effect.runPromiseExit(timeout(5, loser(log, "work")));
  log.push("returned");
  assert.deepEqual(errorsOf(exit), [
    { _tag: tag("Prelude.TimeoutError.TimeoutError") },
  ]);
  assert.deepEqual(log, ["work cleaned up", "returned"]);
});

test("timeout_option is None on time out and keeps other failures", async () => {
  const none = await Effect.runPromise(timeoutOption(5, Effect.never));
  assert.ok(Option.isNone(none));
  const some = await Effect.runPromise(timeoutOption(50, Effect.succeed(1)));
  assert.equal(value(some), 1);
  const failed = await Effect.runPromiseExit(timeoutOption(50, Effect.fail("E")));
  assert.deepEqual(errorsOf(failed), ["E"]);
});
