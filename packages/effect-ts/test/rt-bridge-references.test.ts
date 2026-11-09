import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, References } from "effect";
import { currentCtx, fork, awaitFiber, asExit, type Run } from "@flick-lang/runtime/rt/fiber";
import { scopeRun } from "@flick-lang/runtime/rt/owner";
import * as Deferred from "@flick-lang/runtime/rt/effect/Deferred";

import { fromEffect } from "../src/bridge.ts";

const run = (task: () => unknown): Promise<unknown> => new Promise((resolve, reject) => {
  fork(task, currentCtx()).observe((exit) => exit.cause === undefined ? resolve(exit.value) : reject(new Error(JSON.stringify(exit.cause))));
});
const annotations = () => fromEffect(Effect.service(References.CurrentLogAnnotations));

test("a scoped Effect reference is visible to later native calls and restored at scope close", async () => {
  const result = await run(function* (): Run {
    const inside = yield scopeRun(function* (): Run {
      yield fromEffect(Effect.annotateLogsScoped("request", "7"));
      return yield annotations();
    });
    const outside = yield annotations();
    return [inside, outside];
  });
  assert.deepEqual(result, [{ request: "7" }, {}]);
});

test("overlapping native fibers inherit scoped references and isolate later changes", async () => {
  const result = await run(() => scopeRun(function* (): Run {
    yield fromEffect(Effect.annotateLogsScoped("request", "parent"));
    const ready = Deferred.make();
    const release = Deferred.make();
    const child = fork(() => scopeRun(function* (): Run {
      yield fromEffect(Effect.annotateLogsScoped("request", "child"));
      Deferred.succeed(undefined)(ready);
      yield Deferred.await(release);
      return yield annotations();
    }), currentCtx());
    yield Deferred.await(ready);
    const parent = yield annotations();
    Deferred.succeed(undefined)(release);
    const exit = asExit(yield awaitFiber(child));
    assert.equal(exit.cause, undefined);
    return [parent, exit.value];
  }));
  assert.deepEqual(result, [{ request: "parent" }, { request: "child" }]);
});
