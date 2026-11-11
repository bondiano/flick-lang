import assert from "node:assert/strict";
import { test } from "node:test";

import { fail, raise } from "../src/rt/cause.ts";
import * as Deferred from "../src/rt/effect/Deferred.ts";
import { root, sleep } from "./rt-support.ts";

test("Deferred.into stores a suspended task's outcome once for every waiter", async () => {
  const deferred = Deferred.make();
  let runs = 0;
  const completed = await root(() => Deferred.into(deferred)(function* () {
    runs++;
    yield sleep(0);
    return 42;
  }));
  assert.equal(completed.cause, undefined);
  assert.equal(completed.value, true);
  assert.equal((await root(() => Deferred.await(deferred))).value, 42);
  assert.equal((await root(() => Deferred.await(deferred))).value, 42);
  assert.equal((await root(() => Deferred.into(deferred)(() => { runs++; return 0; }))).value, false);
  assert.equal(runs, 1);
});

test("Deferred.into preserves a typed failure for its waiter", async () => {
  const deferred = Deferred.make();
  const error = { message: "failed" };
  const completed = await root(() => Deferred.into(deferred)(() => raise(fail(error))));
  assert.equal(completed.cause, undefined);
  assert.equal(completed.value, true);
  assert.deepEqual((await root(() => Deferred.await(deferred))).cause, fail(error));
});
