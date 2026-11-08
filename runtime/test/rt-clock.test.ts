// The kernel's clock: sleeps, timeouts and schedules wait on the clock in the
// evidence, and a test clock wakes its sleepers in order as it moves, each
// with what it scheduled run before the clock moves on.
import assert from "node:assert/strict";
import { test } from "node:test";

import { fail } from "../src/rt/cause.ts";
import { CLOCK, TestClock, clock } from "../src/rt/clock.ts";
import { timeout } from "../src/rt/concurrency.ts";
import { Duration, sleep } from "../src/rt/duration.ts";
import { spaced } from "../src/rt/effect/Schedule.ts";
import { type Run, Under, asExit, awaitFiber, currentCtx, extend, fork, raise } from "../src/rt/kernel.ts";
import { retry } from "../src/rt/schedule.ts";
import { root } from "./rt-support.ts";

const HOUR = 3_600_000;

/** `task` on `c`. */
const on = (c: TestClock, task: () => unknown): Under => new Under(extend(currentCtx(), CLOCK, c), task);

test("sleepers wake in order, each with what it started run before the clock moves on", async () => {
  const c = new TestClock();
  const log: Array<string> = [];
  const exit = await root(() =>
    on(c, function* (): Run {
      const sleeper = (name: string, hours: number) =>
        fork(function* (): Run {
          yield sleep(new Duration(hours * HOUR));
          log.push(`${name} at ${clock().millis() / HOUR}`);
          // a fiber the wake starts runs before the next sleeper wakes
          fork(() => void log.push(`${name}'s child`), currentCtx());
          return undefined;
        }, currentCtx());
      const fibers = [sleeper("two", 2), sleeper("one", 1), sleeper("also one", 1)];
      yield* c.run((now) => now + HOUR, BigInt(HOUR) * 1_000_000n);
      log.push(`adjusted to ${clock().millis() / HOUR}`);
      yield* c.run(() => 3 * HOUR, undefined);
      for (const f of fibers) yield awaitFiber(f);
      return [clock().nanos(), clock().monotonic()];
    }),
  );
  assert.deepEqual(log, [
    "one at 1",
    "one's child",
    "also one at 1",
    "also one's child",
    "adjusted to 1",
    "two at 2",
    "two's child",
  ]);
  assert.deepEqual(exit.value, [3n * BigInt(HOUR) * 1_000_000n, 3n * BigInt(HOUR) * 1_000_000n]);
});

test("a timeout and a retry wait for virtual time", async () => {
  const c = new TestClock();
  let calls = 0;
  const exit = await root(() =>
    on(c, function* (): Run {
      const timed = fork(() => timeout(5, () => sleep(new Duration(HOUR))), currentCtx());
      const retried = fork(
        () =>
          retry(spaced(new Duration(HOUR)))(() => {
            calls++;
            return calls < 3 ? raise(fail("again")) : "done";
          }),
        currentCtx(),
      );
      yield* c.run((now) => now + HOUR, undefined);
      const atHour = calls;
      yield* c.run((now) => now + HOUR, undefined);
      assert.equal(atHour, 2);
      assert.notEqual(asExit(yield awaitFiber(timed)).cause, undefined, "timed out");
      assert.equal(asExit(yield awaitFiber(retried)).value, "done");
      return undefined;
    }),
  );
  assert.equal(exit.cause, undefined);
});

test("the live clock sleeps in real time", async () => {
  const started = performance.now();
  const before = clock().monotonic();
  await root(() => sleep(new Duration(5)));
  assert.ok(performance.now() - started >= 4);
  assert.ok(clock().monotonic() - before >= 4_000_000n);
  assert.ok(clock().millis() > 1_700_000_000_000);
});
