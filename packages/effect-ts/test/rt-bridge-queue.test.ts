import assert from "node:assert/strict";
import { test } from "node:test";

import { Channel } from "effect";
import type { Exit } from "@flick-lang/runtime/rt/cause";
import { currentCtx, fork } from "@flick-lang/runtime/rt/fiber";
import { collect, unbounded } from "@flick-lang/runtime/rt/effect/Queue";

import { fromEffect } from "../src/bridge.ts";
import { runIntoQueue } from "../src/effect/Channel.ts";

const root = (task: () => unknown): Promise<Exit> => new Promise((resolve) => {
  fork(task, currentCtx()).observe(resolve);
});

test("an Effect channel offers values and an end to a native queue", async () => {
  const queue = unbounded();
  const written = await root(() => fromEffect(runIntoQueue(queue)(Channel.fromIterable([1, 2, 3]))));
  assert.equal(written.cause, undefined);
  const read = await root(() => collect(queue));
  assert.equal(read.cause, undefined);
  assert.deepEqual(read.value, [1, 2, 3]);
});
