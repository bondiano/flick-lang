import assert from "node:assert/strict";
import { test } from "node:test";
import type { Built, Diagnostic } from "../src/playground/compiler.worker.ts";

type Bridge = typeof import("../src/playground/compiler.ts");

class RunWorker {
  static instances: Array<RunWorker> = [];
  readonly messages: Array<unknown> = [];
  onmessage: ((event: { data: unknown }) => void) | undefined;
  terminated = false;

  constructor() { RunWorker.instances.push(this); }
  postMessage(message: unknown): void { this.messages.push(message); }
  terminate(): void { this.terminated = true; }
  reply(data: unknown): void { this.onmessage?.({ data }); }
}

const built: Built = { files: { "main.js": "export const main = () => {};" }, entry: "main.js", diagnostics: [] };
const warning: Diagnostic = { code: "W001", severity: "warning", message: "kept", at: "1:1", start: 0, end: 1, fixes: [] };

const withBridge = async (name: string, check: (bridge: Bridge) => Promise<void>): Promise<void> => {
  const original = { Worker: globalThis.Worker, document: globalThis.document };
  Object.assign(globalThis, { Worker: RunWorker, document: { getElementById: () => null } });
  RunWorker.instances = [];
  try {
    await check(await import(new URL(`../src/playground/compiler.ts?run-${name}`, import.meta.url).href));
  } finally {
    Object.assign(globalThis, original);
  }
};

test("preparation emits once and never starts a runtime before its result is executed", async () => {
  await withBridge("prepare", async (bridge) => {
    const signal = new AbortController().signal;
    const preparing = bridge.prepareRun("source", "release", signal);
    const compiler = RunWorker.instances[0]!;
    assert.deepEqual(compiler.messages, [{ action: "build", source: "source", profile: "release", id: 1 }]);
    assert.equal(RunWorker.instances.length, 1);
    compiler.reply({ id: 1, result: built });
    const prepared = await preparing;
    assert.equal(RunWorker.instances.length, 1);
    const execution = bridge.runPrepared(prepared, signal);
    const runtime = RunWorker.instances[1]!;
    assert.deepEqual(runtime.messages, [{ files: built.files, entry: "main.js" }]);
    runtime.reply({ stream: "stdout", text: "hello" });
    runtime.reply({ stream: "exit", code: 0 });
    assert.deepEqual(await execution, { lines: [{ stream: "stdout", text: "hello" }], status: "exit", detail: "0", diagnostics: [] });
    assert.equal(compiler.messages.length, 1);
    assert.equal(runtime.terminated, true);
  });
});

test("canceling queued preparation removes the build without starting a runtime", async () => {
  await withBridge("cancel-queued", async (bridge) => {
    const busy = bridge.check("busy", new AbortController().signal);
    const controller = new AbortController();
    const preparing = bridge.prepareRun("canceled", "debug", controller.signal);
    const rejected = assert.rejects(preparing, { name: "AbortError" });
    controller.abort();
    await rejected;
    const compiler = RunWorker.instances[0]!;
    compiler.reply({ id: 1, result: [] });
    await busy;
    assert.deepEqual(compiler.messages, [{ action: "check", source: "busy", id: 1 }]);
    assert.equal(RunWorker.instances.length, 1);
  });
});

test("a canceled legacy run discards its late build reply", async () => {
  await withBridge("cancel-active", async (bridge) => {
    const controller = new AbortController();
    const execution = bridge.run("source", "debug", controller.signal);
    controller.abort();
    assert.equal((await execution).status, "stopped");
    RunWorker.instances[0]!.reply({ id: 1, result: built });
    assert.equal(RunWorker.instances.length, 1);
  });
});

test("a prepared result cannot execute after a newer preview generation replaces it", async () => {
  await withBridge("stale", async (bridge) => {
    const signal = new AbortController().signal;
    const preparing = bridge.prepareRun("older", "debug", signal);
    const compiler = RunWorker.instances[0]!;
    compiler.reply({ id: 1, result: { ...built, diagnostics: [warning] } });
    const prepared = await preparing;
    bridge.closePreview();
    assert.deepEqual(await bridge.runPrepared(prepared, signal), { lines: [], status: "stopped", detail: "", diagnostics: [warning] });
    assert.equal(RunWorker.instances.length, 1);
  });
});

test("prepared refusal and missing-main results retain diagnostics without starting workers", async () => {
  await withBridge("refusal", async (bridge) => {
    const signal = new AbortController().signal;
    for (const [files, status] of [[null, "invalid"], [built.files, "no-main"]] as const) {
      const preparing = bridge.prepareRun("source", "debug", signal);
      const compiler = RunWorker.instances[0]!;
      compiler.reply({ id: compiler.messages.length, result: { files, entry: null, diagnostics: [warning] } });
      assert.deepEqual(await bridge.runPrepared(await preparing, signal), { lines: [], status, detail: "", diagnostics: [warning] });
      assert.equal(RunWorker.instances.length, 1);
    }
  });
});

test("canceling prepared execution terminates its worker and retains collected output", async () => {
  await withBridge("cancel-execution", async (bridge) => {
    const controller = new AbortController();
    const preparing = bridge.prepareRun("source", "debug", controller.signal);
    RunWorker.instances[0]!.reply({ id: 1, result: built });
    const execution = bridge.runPrepared(await preparing, controller.signal);
    const runtime = RunWorker.instances[1]!;
    runtime.reply({ stream: "stdout", text: "before stop" });
    controller.abort();
    assert.deepEqual(await execution, { lines: [{ stream: "stdout", text: "before stop" }], status: "stopped", detail: "", diagnostics: [] });
    assert.equal(runtime.terminated, true);
  });
});

test("suite execution and listing use disposable workers and never the main preview", async () => {
  await withBridge("tests", async (bridge) => {
    const suites: Built = { ...built, tests: { module: "Main", entry: "Main.js" } };
    for (const list of [true, false]) {
      const execution = bridge.runTestsBuilt(suites, { list, filter: "sum" }, new AbortController().signal);
      const worker = RunWorker.instances.at(-1)!;
      assert.deepEqual(worker.messages, [{ files: suites.files, entry: "Main.js", tests: { module: "Main", options: { list, filter: "sum" } } }]);
      worker.reply({ stream: "stdout", text: "Main > sum\n" });
      worker.reply({ stream: "exit", code: 0 });
      assert.equal((await execution).detail, "0");
      assert.equal(worker.terminated, true);
    }
    assert.equal(RunWorker.instances.length, 2);
  });
});

test("suite cancellation waits for hook and fixture cleanup before terminating", async () => {
  await withBridge("tests-stop", async (bridge) => {
    const controller = new AbortController();
    const execution = bridge.runTestsBuilt({ ...built, tests: { module: "Main", entry: "Main.js" } }, {}, controller.signal);
    const worker = RunWorker.instances[0]!;
    controller.abort();
    assert.equal(worker.terminated, false);
    assert.deepEqual(worker.messages.at(-1), { action: "stop" });
    worker.reply({ stream: "stdout", text: "cleanup\n" });
    worker.reply({ stream: "exit", code: 130 });
    assert.deepEqual(await execution, { lines: [{ stream: "stdout", text: "cleanup\n" }], status: "stopped", detail: "", diagnostics: [] });
    assert.equal(worker.terminated, true);
  });
});
