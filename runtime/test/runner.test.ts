import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect } from "effect";

import { run } from "../src/runner/browser.ts";

test("the browser runner gives main its input, whether it is pure or an effect", async () => {
  const seen: Array<unknown> = [];
  await run((input: unknown) => {
    seen.push(input);
  }, "pure");
  await run((input: unknown) => Effect.sync(() => seen.push(input)), { id: "root" });
  await run(() => {
    seen.push("no input");
  });
  assert.deepEqual(seen, ["pure", { id: "root" }, "no input"]);
});

test("the browser runner refuses what is not a function", async () => {
  await assert.rejects(run({ main: true }), TypeError);
});
