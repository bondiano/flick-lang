import assert from "node:assert/strict";
import { test } from "node:test";

class WorkerHarness {
  static instances: WorkerHarness[] = [];
  readonly messages: Array<{ id: number; action: string; source?: string }> = [];
  onmessage: ((event: { data: { id: number; result: unknown } }) => void) | undefined;
  onerror: ((event: { error: Error; message: string }) => void) | undefined;
  terminated = false;

  constructor() { WorkerHarness.instances.push(this); }
  postMessage(message: { id: number; action: string; source?: string }): void { this.messages.push(message); }
  terminate(): void { this.terminated = true; }
  reply(id: number, result: unknown): void { this.onmessage?.({ data: { id, result } }); }
  crash(): void { this.onerror?.({ error: new Error("compiler offline"), message: "compiler offline" }); }
}

const compiled = { debug: { code: "", diagnostics: [] }, release: { code: "", diagnostics: [] } };

test("canceled work waiting behind a busy compiler is never executed", { timeout: 1000 }, async () => {
  const original = globalThis.Worker;
  globalThis.Worker = WorkerHarness as unknown as typeof Worker;
  WorkerHarness.instances = [];
  try {
    const bridge = await import(new URL("../src/playground/compiler.ts?backpressure", import.meta.url).href);
    const firstController = new AbortController();
    const first = bridge.compile("first", firstController.signal);
    const firstCanceled = assert.rejects(first, { name: "AbortError" });
    firstController.abort();
    await firstCanceled;
    const worker = WorkerHarness.instances[0];
    assert.ok(worker);

    const staleController = new AbortController();
    const stale = bridge.compile("superseded", staleController.signal);
    const staleCanceled = assert.rejects(stale, { name: "AbortError" });
    staleController.abort();
    await staleCanceled;
    const latest = bridge.compile("current", new AbortController().signal);
    assert.deepEqual(worker.messages.map(message => message.source), ["first"]);

    worker.reply(worker.messages[0]!.id, compiled);
    assert.deepEqual(worker.messages.map(message => message.source), ["first", "current"]);
    worker.reply(worker.messages[1]!.id, compiled);
    await latest;
  } finally {
    globalThis.Worker = original;
  }
});

test("a crashed compiler rejects active and queued callers and permits explicit recovery", { timeout: 1000 }, async () => {
  const original = globalThis.Worker;
  globalThis.Worker = WorkerHarness as unknown as typeof Worker;
  WorkerHarness.instances = [];
  try {
    const bridge = await import(new URL("../src/playground/compiler.ts?recovery", import.meta.url).href);
    const loading = bridge.load();
    const checking = bridge.compile("queued", new AbortController().signal);
    const failures = Promise.all([
      assert.rejects(loading, /compiler offline/),
      assert.rejects(checking, /compiler offline/),
    ]);
    const failed = WorkerHarness.instances[0];
    assert.ok(failed);
    failed.crash();
    await failures;
    assert.equal(failed.terminated, true);

    const recovered = bridge.load();
    assert.equal(WorkerHarness.instances.length, 2);
    const replacement = WorkerHarness.instances[1];
    assert.ok(replacement);
    assert.equal(replacement.messages.length, 1);
    assert.equal(replacement.messages[0]!.action, "load");
    replacement.reply(replacement.messages[0]!.id, "0.1.0");
    await recovered;
  } finally {
    globalThis.Worker = original;
  }
});
