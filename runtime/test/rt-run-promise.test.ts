// `runPromise` of Flick's own runtime: what a page awaits on the server.
import assert from "node:assert/strict";
import { test } from "node:test";

import { fail, raise } from "../src/rt/cause.ts";
import { RuntimeFailure, runPromise } from "../src/rt/facade.ts";

test("a task's value resolves, and a failure rejects with its report", async () => {
  assert.equal(await runPromise(() => 1 + 1), 2);
  await assert.rejects(runPromise(() => raise(fail("boom"))), (error) => error instanceof RuntimeFailure);
  await assert.rejects(runPromise(42), { name: "TypeError" });
});
