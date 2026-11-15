import assert from "node:assert/strict";
import { test } from "node:test";
import { setImmediate } from "node:timers/promises";

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
    for (const worker of WorkerHarness.instances) worker.crash();
    Object.assign(globalThis, original);
    t.mock.timers.reset();
  }
});

class LessonWorker {
  static instances: Array<LessonWorker> = [];
  readonly messages: Array<unknown> = [];
  onmessage: ((event: { data: unknown }) => void) | undefined;
  onerror: ((event: { error: Error; message: string }) => void) | undefined;
  terminated = false;

  constructor() { LessonWorker.instances.push(this); }
  postMessage(message: unknown): void { this.messages.push(message); }
  terminate(): void { this.terminated = true; }
  reply(data: unknown): void { this.onmessage?.({ data }); }
  crash(): void { this.onerror?.({ error: new Error("compiler offline"), message: "compiler offline" }); }
  request(): { id: number; action: string } {
    const message = this.messages.at(-1);
    assert.ok(message && typeof message === "object" && "id" in message && "action" in message);
    assert.ok(typeof message.id === "number" && typeof message.action === "string");
    return { id: message.id, action: message.action };
  }
}

for (const outcome of ["passed", "output", "type"] as const) {
  const title = {
    passed: "a re-export lesson executes its declaration build once within the original deadline",
    output: "the original lesson deadline still terminates cached execution",
    type: "a rejected lesson signature leaves the existing preview alone",
  }[outcome];
  test(title, async (t) => {
    const original = { Worker: globalThis.Worker, document: globalThis.document };
    const rules = { export: "length", argument_types: ["string"], return_type: outcome === "type" ? "string" : "number", output: "5" };
    let previewCloses = 0;
    Object.assign(globalThis, {
      Worker: LessonWorker,
      document: { getElementById: (id: string) => id === "flick-lesson"
        ? { textContent: JSON.stringify({ check: rules }) }
        : id === "fp-frame" ? { replaceChildren: () => { previewCloses += 1; } } : null },
    });
    LessonWorker.instances = [];
    t.mock.timers.enable({ apis: ["setTimeout"] });
    let pending: Promise<string> | undefined;
    try {
      const { checkLesson } = await import("../src/playground/lesson.ts");
      pending = checkLesson("re-export source");
      const compiler = LessonWorker.instances[0]!;
      const checking = compiler.request();
      assert.equal(checking.action, "check");
      t.mock.timers.tick(3_000);
      compiler.reply({ id: checking.id, result: [] });
      await setImmediate();
      const compiling = compiler.request();
      assert.equal(compiling.action, "compile");
      const built = { files: { "main.js": "export const main = () => console.log(5);" }, entry: "main.js", diagnostics: [] };
      t.mock.timers.tick(6_000);
      compiler.reply({ id: compiling.id, result: {
        code: built.files["main.js"],
        dts: 'export { length } from "./String.js";',
        declarations: { "String.d.ts": "export declare const length: (text: string) => number;" },
        declarationEntry: "Main.d.ts",
        diagnostics: [],
        built,
      } });
      await setImmediate();
      assert.equal(compiler.messages.length, 2, "declaration compilation must not be followed by a second build");
      if (outcome === "type") {
        assert.equal(await pending, "type");
        assert.equal(LessonWorker.instances.length, 1);
        assert.equal(previewCloses, 0);
        return;
      }
      const runtime = LessonWorker.instances[1]!;
      assert.ok(runtime, "the checked compilation must be executed");
      assert.equal(previewCloses, 1);
      assert.deepEqual(runtime.messages, [{ files: built.files, entry: built.entry }]);
      if (outcome === "passed") {
        t.mock.timers.tick(500);
        runtime.reply({ stream: "stdout", text: "5\n" });
        runtime.reply({ stream: "exit", code: 0 });
        assert.equal(await pending, "passed");
      } else {
        t.mock.timers.tick(1_001);
        assert.equal(await pending, "output");
      }
      assert.equal(runtime.terminated, true);
    } finally {
      t.mock.timers.tick(10_001);
      await pending;
      for (const worker of LessonWorker.instances) worker.crash();
      Object.assign(globalThis, original);
      t.mock.timers.reset();
    }
  });
}
