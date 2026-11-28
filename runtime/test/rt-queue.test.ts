import assert from "node:assert/strict";
import { test } from "node:test";

import { bounded, flickTake, offer, offerAll, type Queue } from "../src/rt/effect/Queue.ts";
import { fail } from "../src/rt/cause.ts";
import { isRun, type Run } from "../src/rt/fiber.ts";
import { root } from "./rt-support.ts";

const ITEMS = 5000;

function* produceOneByOne(q: Queue): Run {
  for (let i = 1; i <= ITEMS; i++) yield offer(i)(q);
  return undefined;
}

function* produceInBatches(q: Queue): Run {
  for (let i = 1; i <= ITEMS; i += 3) {
    const batch = [i, i + 1, i + 2].filter((n) => n <= ITEMS);
    yield offerAll(batch)(q);
  }
  return undefined;
}

/** The value of a `Some`; fails on anything else. */
const someNumber = (option: unknown): number => {
  if (typeof option === "object" && option !== null && "_tag" in option && option._tag === "Some" && "value" in option && typeof option.value === "number") {
    return option.value;
  }
  return assert.fail(`expected Some of a number, got ${String(option)}`);
};

function* consume(q: Queue): Run {
  let sum = 0;
  for (let left = ITEMS; left > 0; left--) {
    sum += someNumber(yield flickTake(q));
  }
  return sum;
}

// An offer that found the queue full used to register only after a pause of the
// driver, so a take in between left free room, a waiting offer and a waiting taker.
for (const capacity of [1, 2]) {
  for (const [name, produce] of [
    ["offer", produceOneByOne],
    ["offerAll", produceInBatches],
  ] as const) {
    test(`two fibers hand ${ITEMS} messages through bounded(${capacity}) with ${name}`, { timeout: 10_000 }, async () => {
      const q = bounded(capacity);
      const [produced, consumed] = await Promise.all([root(() => produce(q)), root(() => consume(q))]);
      assert.equal(produced.cause, undefined);
      assert.equal(consumed.cause, undefined);
      assert.equal(consumed.value, (ITEMS * (ITEMS + 1)) / 2);
    });
  }
}

/**
 * Steps `run` up to the wait it yields when the queue is full, does `meanwhile`
 * as another fiber would during a pause of the driver, then steps the wait:
 * it must settle without suspending; what `run` returns.
 */
const overtaken = (run: Run, meanwhile: () => void): unknown => {
  const first = run.next();
  assert.equal(first.done, false);
  const wait = first.value;
  assert.ok(isRun(wait), "a full queue makes the offer wait");
  meanwhile();
  const settled = wait.next();
  assert.equal(settled.done, true, "the wait settles in the step that would register it");
  const last = run.next(settled.value);
  assert.equal(last.done, true);
  return last.value;
};

test("an offer overtaken by a take goes in instead of waiting", () => {
  const q = bounded(1);
  q.offerNow(1);
  assert.equal(overtaken(offer(2)(q), () => q.takeNow()), true);
  assert.deepEqual(q.messages, [2]);
  assert.equal(q.offers.length, 0);
});

test("offerAll overtaken by takes puts in the rest instead of waiting", () => {
  const q = bounded(3);
  q.offerAllNow([1, 2, 3]);
  assert.deepEqual(
    overtaken(offerAll([4, 5])(q), () => {
      q.takeNow();
      q.takeNow();
    }),
    [],
  );
  assert.deepEqual(q.messages, [3, 4, 5]);
  assert.equal(q.offers.length, 0);
});

test("an offer overtaken by a shutdown fails instead of waiting", () => {
  const q = bounded(1);
  q.offerNow(1);
  assert.equal(overtaken(offer(2)(q), () => q.shutdown()), false);
  assert.equal(q.offers.length, 0);
});

test("offerAll overtaken by an end gives back what it did not add", () => {
  const q = bounded(1);
  q.offerNow(1);
  assert.deepEqual(overtaken(offerAll([2, 3])(q), () => q.failCause(fail("closed"))), [2, 3]);
  assert.equal(q.offers.length, 0);
});
