import assert from "node:assert/strict";
import { test } from "node:test";

import { CLOCK, TestClock } from "../src/rt/clock.ts";
import { flickCurrentTimeMillis } from "../src/rt/effect/Clock.ts";
import { flickUnsafeFromEpochMillis, toEpochMillis } from "../src/rt/effect/DateTime.ts";
import { Under, currentCtx, extend } from "../src/rt/fiber.ts";
import { root } from "./rt-support.ts";

const now = () => toEpochMillis(flickUnsafeFromEpochMillis(flickCurrentTimeMillis()));

test("DateTime.now reads the clock of the current fiber", async () => {
  const exit = await root(() => new Under(extend(currentCtx(), CLOCK, new TestClock()), now));
  assert.equal(exit.cause, undefined);
  assert.equal(exit.value, 0);
});
