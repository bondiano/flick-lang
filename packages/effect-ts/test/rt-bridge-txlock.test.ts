import assert from "node:assert/strict";
import { test } from "node:test";

import { currentCtx, fork, type Run } from "@flick-lang/runtime/rt/fiber";

import { fromEffect } from "../src/bridge.ts";
import * as Lock from "../src/effect/TxReentrantLock.ts";

test("one native fiber can reenter a transactional lock across Effect calls", { timeout: 1000 }, async () => {
  const counts = await new Promise<unknown>((resolve, reject) => {
    fork(function* (): Run {
      const lock: unknown = yield fromEffect(Lock.make());
      assert.ok(Lock.isTxReentrantLock(lock));
      const first = yield fromEffect(Lock.acquireWrite(lock));
      const second = yield fromEffect(Lock.acquireWrite(lock));
      const left = yield fromEffect(Lock.releaseWrite(lock));
      const empty = yield fromEffect(Lock.releaseWrite(lock));
      return [first, second, left, empty];
    }, currentCtx()).observe((exit) => exit.cause === undefined ? resolve(exit.value) : reject(new Error(JSON.stringify(exit.cause))));
  });
  assert.deepEqual(counts, [1, 2, 1, 0]);
});
