import assert from "node:assert/strict";
import { test } from "node:test";

/** A busy compiler never replies until the test releases it. */
class WorkerHarness {
  static current: WorkerHarness;
  messages: Array<{ id: number }> = [];
  onmessage?: (event: { data: { id: number; result: unknown } }) => void;
  constructor() { WorkerHarness.current = this; }
  postMessage(message: { id: number }): void { this.messages.push(message); }
  terminate(): void {}
  reply(result: unknown): void {
    const message = this.messages.at(-1);
    assert.ok(message);
    this.onmessage?.({ data: { id: message.id, result } });
  }
}

test("lesson timeout and compiler errors never persist completion", async (t) => {
  const original = { Worker: globalThis.Worker, document: globalThis.document, localStorage: globalThis.localStorage };
  let writes = 0;
  Object.assign(globalThis, {
    Worker: WorkerHarness,
    document: { getElementById: () => ({ textContent: JSON.stringify({ id: "signatures/02", check: { export: "size_of", argument_types: ["string"], return_type: "number", output: "6" } }) }) },
    localStorage: { setItem: () => { writes += 1; } },
  });
  t.mock.timers.enable({ apis: ["setTimeout"] });
  try {
    const { checkLesson } = await import("../src/playground/lesson.ts");
    const timedOut = checkLesson("busy");
    t.mock.timers.tick(10_001);
    assert.equal(await timedOut, "failed");
    WorkerHarness.current.reply([]);
    const errors = checkLesson("broken");
    WorkerHarness.current.reply([{ severity: "error" }]);
    assert.equal(await errors, "fix-errors");
    assert.equal(writes, 0);
  } finally {
    Object.assign(globalThis, original);
    t.mock.timers.reset();
  }
});
