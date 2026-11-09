import assert from "node:assert/strict";
import { test } from "node:test";

import { Duration as EffectDuration, Effect, Schedule } from "effect";

import type { Exit } from "@flick-lang/runtime/rt/cause";
import { Duration, durationFormat } from "@flick-lang/runtime/rt/duration";
import { currentCtx, fork } from "@flick-lang/runtime/rt/fiber";
import { repeat } from "@flick-lang/runtime/rt/schedule";
import { Schedule as NativeSchedule, outOf } from "@flick-lang/runtime/rt/effect/Schedule";

import { scheduleFromEffect } from "../src/bridge.ts";

const root = (task: () => unknown): Promise<Exit> => new Promise((resolve) => {
  fork(task, currentCtx()).observe(resolve);
});

test("an Effect schedule crosses into native repeat with its completion and output", async () => {
  const schedule = scheduleFromEffect(Schedule.recurs(2));
  assert.ok(schedule instanceof NativeSchedule);
  let runs = 0;
  const exit = await root(() => repeat(schedule)(() => ++runs));
  assert.equal(exit.cause, undefined);
  assert.equal(runs, 3);
  assert.equal(exit.value, 2);
});

test("a schedule output duration crosses with the delay as a native value", async () => {
  const effectSchedule = Schedule.fromStep(Effect.sync(() => () => {
    const out: [EffectDuration.Duration, EffectDuration.Duration] = [EffectDuration.seconds(1), EffectDuration.zero];
    return Effect.succeed(out);
  }));
  const native = scheduleFromEffect(effectSchedule);
  assert.ok(native instanceof NativeSchedule);
  const exit = await root(() => native.start()(Date.now(), undefined));
  assert.equal(exit.cause, undefined);
  const out = outOf(exit.value);
  assert.ok(out[0] instanceof Duration);
  assert.equal(out[0].millis, 1000);
});

test("a host duration returned by a polymorphic Effect API formats on the native runtime", () => {
  assert.equal(durationFormat(EffectDuration.millis(1500)), "1s 500ms");
});
