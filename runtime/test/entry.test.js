import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, appFail, runMain, stdoutLine } from "../src/index.js";

test("runMain maps success and AppError to exit codes", async (t) => {
  const written = [];
  t.mock.method(process.stdout, "write", (s) => written.push(["out", s]));
  t.mock.method(process.stderr, "write", (s) => written.push(["err", s]));
  const line = (text) => Effect.sync(() => stdoutLine(text));
  try {
    await runMain(() => line("a"));
    assert.equal(process.exitCode, 0);
    await runMain(() => Effect.gen(function* () {
      yield* line("b");
      yield* appFail("boom");
      yield* line("never");
    }));
    assert.equal(process.exitCode, 1);
  } finally {
    process.exitCode = 0;
  }
  assert.deepEqual(written, [
    ["out", "a\n"],
    ["out", "b\n"],
    ["err", "error: boom\n"],
  ]);
});
