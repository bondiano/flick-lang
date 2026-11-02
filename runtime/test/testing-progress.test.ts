import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect } from "effect";

import { runTests, testEffect } from "../src/stdlib/testing.ts";

test("the live test line appears before a slow test finishes and clears before its result", async () => {
  let output = "";
  const before = process.exitCode;
  try {
    const running = runTests([["Sample", [testEffect("slow", Effect.sleep(150))]]], {
      progress: true,
      write: (text) => { output += text; },
    });
    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.match(output, /0\/1 complete · 1 running · Sample > slow/);
    assert.doesNotMatch(output, /✓ \[1\/1\]/);
    await running;
    assert.ok(output.includes("\r\x1b[2K  ✓ [1/1] Sample > slow"));
    assert.match(output, /1 passed · 0 failed · 1 total/);
  } finally {
    process.exitCode = before;
  }
});
