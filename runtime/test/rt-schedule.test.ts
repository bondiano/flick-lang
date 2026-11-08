// Schedules on the kernel, run by `repeat` and `retry`: what
// tests/run/stdlib/effect/schedule prints on Effect, which the case itself
// cannot print here while `Duration` is on Effect.
import assert from "node:assert/strict";
import { test } from "node:test";

import { type Option, none, some } from "../src/core/data.ts";
import { type Exit, die, fail, raise } from "../src/rt/cause.ts";
import { Duration } from "../src/rt/duration.ts";
import {
  type Metadata,
  type Schedule,
  addDelay,
  concat,
  concatResult,
  during,
  exponential,
  fibonacci,
  flickCurrentMetadata,
  flickUpTo,
  forever,
  identity,
  max,
  min,
  modifyDelay,
  passthrough,
  recurs,
  spaced,
  tap,
  while as whileIt,
} from "../src/rt/effect/Schedule.ts";
import { repeat, repeatOrElse, retry, retryOrElse, schedule, scheduleFrom } from "../src/rt/schedule.ts";
import { root } from "./rt-support.ts";

const ms = (n: number): Duration => new Duration(n);

const upTo = (times: number) => flickUpTo({ duration: none, times: some(times) });

const valueOf = (exit: Exit): unknown => {
  assert.equal(exit.cause, undefined, "succeeded");
  return exit.value;
};

/** How many times `s` runs a task. */
const count = async (s: Schedule): Promise<number> => {
  let calls = 0;
  valueOf(await root(() => repeat(s)(() => ++calls)));
  return calls;
};

/** The delays of the first steps of `s`, in milliseconds. */
const delays = async (s: Schedule): Promise<Array<number>> => {
  const seen: Array<number> = [];
  const traced = tap((m: Metadata) => {
    seen.push(m.output instanceof Duration ? m.output.millis : Number.NaN);
  })(s);
  valueOf(await root(() => repeat(upTo(3)(traced))(() => 0)));
  return seen;
};

test("schedules recur as many times as Effect's do", async () => {
  assert.equal(await count(recurs(2)), 3);
  assert.equal(await count(upTo(2)(forever)), 3);
  assert.equal(await count(upTo(2)(spaced(ms(1)))), 3);
  assert.equal(await count(max([recurs(1), recurs(3)])), 2);
  assert.equal(await count(min([recurs(1), recurs(3)])), 4);
  assert.equal(await count(concat(recurs(2))(recurs(1))), 4);
  assert.equal(await count(whileIt((m: Metadata) => m.attempt < 3)(forever)), 3);
  assert.equal(await count(addDelay(() => ms(1))(recurs(1))), 2);
  assert.equal(await count(upTo(2)(modifyDelay(() => ms(0))(spaced(ms(10_000))))), 3);
});

test("the delays of a schedule are its outputs", async () => {
  assert.deepEqual(await delays(exponential(ms(1), 3)), [1, 3, 9, 27]);
  assert.deepEqual(await delays(fibonacci(ms(1))), [1, 1, 2, 3]);
  assert.equal((await delays(during(ms(1000)))).length, 4);
});

/** The attempt of the step that runs the task again. */
const attempt = (): unknown => Reflect.get(Object(flickCurrentMetadata()), "attempt");

test("repeat gives the schedule's last output", async () => {
  const last = valueOf(await root(() => repeat(concatResult(recurs(1))(recurs(1)))(() => 0)));
  assert.deepEqual(last, { _tag: "Success", success: 1 });
  assert.equal(valueOf(await root(() => repeat(passthrough(recurs(2)))(() => 7))), 7);
  assert.equal(valueOf(await root(() => repeat(upTo(1)(identity()))(() => "echo"))), "echo");
  assert.equal(valueOf(await root(() => repeat(recurs(2))(attempt))), 2);
  assert.equal(valueOf(await root(() => schedule(recurs(2))(() => undefined))), 2);
  assert.equal(valueOf(await root(() => scheduleFrom(5, passthrough(recurs(1)))(() => 6))), 6);
});

/** A task that fails with its call's number on the first `failures` calls. */
const flaky = (failures: number): { calls: () => number; task: () => unknown } => {
  let calls = 0;
  return {
    calls: () => calls,
    task: () => {
      calls++;
      return calls <= failures ? raise(fail(calls)) : `ok after ${calls}`;
    },
  };
};

test("retry runs a task again after each typed failure while the schedule recurs", async () => {
  assert.equal(valueOf(await root(() => retry(recurs(5))(flaky(2).task))), "ok after 3");
  const exhausted = await root(() => retry(recurs(2))(flaky(10).task));
  assert.deepEqual(exhausted.cause?.reasons, [{ _tag: "Fail", error: 3 }]);
  const other = flaky(10);
  const fallback = await root(() => retryOrElse(recurs(1), (error, out) => [error, out])(other.task));
  assert.deepEqual(valueOf(fallback), [2, 1]);
  const recovered = await root(() =>
    repeatOrElse(recurs(3), (_error: unknown, step: Option<Metadata>) => (step._tag === "Some" ? 1 : 0))(() => raise(fail(0))),
  );
  assert.equal(valueOf(recovered), 0);
});

test("a defect is never retried", async () => {
  let runs = 0;
  const exit = await root(() =>
    retry(recurs(5))(() => {
      runs++;
      return raise(die("division by zero"));
    }),
  );
  assert.equal(runs, 1);
  assert.equal(exit.cause?.reasons[0]?._tag, "Die");
});
