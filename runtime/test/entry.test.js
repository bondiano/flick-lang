import assert from "node:assert/strict";
import { test } from "node:test";

import { appFail, runMain, seq, stdoutLine } from "../src/index.js";

test("runMain maps success and AppError to exit codes", async (t) => {
  const written = [];
  t.mock.method(process.stdout, "write", (s) => written.push(["out", s]));
  t.mock.method(process.stderr, "write", (s) => written.push(["err", s]));
  try {
    await runMain(() => seq([stdoutLine("a")]));
    assert.equal(process.exitCode, 0);
    await runMain(() => seq([stdoutLine("b"), appFail("boom"), stdoutLine("never")]));
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
