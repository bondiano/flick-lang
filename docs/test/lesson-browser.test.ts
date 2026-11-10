import assert from "node:assert/strict";
import { test } from "node:test";

import { WorkerHarness } from "./helpers/compiler-worker.ts";

test("lesson timeout and compiler errors never persist completion", async (t) => {
  const original = { Worker: globalThis.Worker, document: globalThis.document, localStorage: globalThis.localStorage };
  let writes = 0;
  Object.assign(globalThis, {
    Worker: WorkerHarness,
    document: { getElementById: () => ({ textContent: JSON.stringify({ check: { export: "size_of", argument_types: ["string"], return_type: "number", output: "6" } }) }) },
    localStorage: { setItem: () => { writes += 1; } },
  });
  t.mock.timers.enable({ apis: ["setTimeout"] });
  try {
    WorkerHarness.instances = [];
    const { checkLesson } = await import("../src/playground/lesson.ts");
    const timedOut = checkLesson("busy");
    t.mock.timers.tick(10_001);
    assert.equal(await timedOut, "failed");
    const worker = WorkerHarness.instances[0];
    assert.ok(worker);
    const first = worker.messages[0];
    assert.ok(first);
    worker.reply(first.id, []);
    const errors = checkLesson("broken");
    const latest = worker.messages.at(-1);
    assert.ok(latest);
    worker.reply(latest.id, [{ severity: "error" }]);
    assert.equal(await errors, "fix-errors");
    assert.equal(writes, 0);
  } finally {
    Object.assign(globalThis, original);
    t.mock.timers.reset();
  }
});
