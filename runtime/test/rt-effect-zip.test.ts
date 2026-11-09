import assert from "node:assert/strict";
import { test } from "node:test";

import { fail, raise } from "../src/rt/cause.ts";
import { zip, zipWith } from "../src/rt/effect/Effect.ts";
import { root } from "./rt-support.ts";

test("zip runs the left task before the right and keeps both results", async () => {
  const seen: Array<string> = [];
  const exit = await root(() => zip(() => { seen.push("right"); return 2; })(() => { seen.push("left"); return 1; }));
  assert.equal(exit.cause, undefined);
  assert.deepEqual(seen, ["left", "right"]);
  assert.deepEqual(exit.value, [1, 2]);
  const combined = await root(() => zipWith(() => 3, (a, b) => Number(a) + Number(b))(() => 4));
  assert.equal(combined.value, 7);
});

test("zip does not start the right task after a failure on the left", async () => {
  const seen: Array<string> = [];
  const exit = await root(() => zip(() => { seen.push("right"); return 2; })(() => raise(fail("boom"))));
  assert.deepEqual(seen, []);
  assert.deepEqual(exit.cause?.reasons, [{ _tag: "Fail", error: "boom" }]);
});
