import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { test } from "node:test";

import { Effect } from "effect";

import { appFail, capabilities, runMain, stdoutLine } from "../src/index.ts";

const fixture = new URL("../fixtures/entry.ts", import.meta.url).pathname;

interface Run {
  readonly code: number | null;
  readonly stdout: string;
  readonly stderr: string;
}

interface RunOptions {
  readonly signal?: NodeJS.Signals;
  readonly env?: Readonly<Record<string, string>>;
}

/** Runs a fixture program; with `signal`, sends it once the program is ready. */
const run = (name: string, { signal, env }: RunOptions = {}): Promise<Run> =>
  new Promise((resolve) => {
    const child = spawn(process.execPath, [fixture, name], {
      env: { ...process.env, ...env },
    });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk: Buffer) => {
      stdout += chunk;
      if (signal && stdout.includes("ready\n")) child.kill(signal);
    });
    child.stderr.on("data", (chunk: Buffer) => {
      stderr += chunk;
    });
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });

test("success exits 0 after cleanup", async () => {
  const r = await run("success");
  assert.deepEqual([r.code, r.stdout, r.stderr], [0, "acquired\ndone\nreleased\n", ""]);
});

test("main failing with AppError exits 1 after cleanup", async () => {
  const r = await run("app-error");
  assert.deepEqual([r.code, r.stdout, r.stderr], [1, "acquired\nreleased\n", "error: config is missing\n"]);
});

test("a defect exits 1 with its cause, unlike a typed failure", async () => {
  const r = await run("defect");
  assert.equal(r.code, 1);
  assert.equal(r.stdout, "acquired\nreleased\n");
  assert.match(r.stderr, /Error: boom/);
  assert.doesNotMatch(r.stderr, /^error: /);
});

test("SIGINT with a successful cleanup exits 130", async () => {
  const r = await run("signal", { signal: "SIGINT" });
  assert.deepEqual([r.code, r.stdout, r.stderr], [130, "acquired\nready\nreleased\n", ""]);
});

test("SIGTERM with a successful cleanup exits 143", async () => {
  const r = await run("signal", { signal: "SIGTERM" });
  assert.deepEqual([r.code, r.stdout], [143, "acquired\nready\nreleased\n"]);
});

test("SIGINT with a failing cleanup exits 1", async () => {
  const r = await run("signal-failing-cleanup", { signal: "SIGINT" });
  assert.equal(r.code, 1);
  assert.match(r.stderr, /close failed/);
});

test("a slow shutdown is reported while cleanup runs", async () => {
  const r = await run("slow-cleanup", { signal: "SIGINT", env: { FLICK_SHUTDOWN_NOTICE_MS: "20" } });
  assert.equal(r.code, 130);
  assert.equal(r.stdout, "acquired\nready\nreleased\n");
  assert.equal(r.stderr, "shutting down: waiting for cleanup to finish\n");
});

test("a defect of a supervisor fails the run", async () => {
  const r = await run("supervisor-defect");
  assert.equal(r.code, 1);
  assert.match(r.stderr, /a supervisor failed[\s\S]*supervisor broke/);
});

test("another Effect release is refused with exit code 2", async () => {
  const r = await run("old-effect");
  assert.deepEqual([r.code, r.stderr], [2, "error: the Flick runtime requires effect 4.0.5, found 4.1.0\n"]);
});

test("the capability check names the version and missing operations", () => {
  assert.deepEqual(capabilities(), []);
  const problems = capabilities("4.1.0", { Effect: {} });
  assert.equal(problems[0], "the Flick runtime requires effect 4.0.5, found 4.1.0");
  assert.ok(problems.includes("effect is missing Effect.gen"));
  assert.ok(problems.includes("effect is missing Scope.make"));
});

const line = (text: string) => Effect.sync(() => stdoutLine(text));

test("runMain in process maps success and AppError to exit codes", async (t) => {
  const written: Array<[string, string]> = [];
  t.mock.method(process.stdout, "write", (s: string) => written.push(["out", s]) > 0);
  t.mock.method(process.stderr, "write", (s: string) => written.push(["err", s]) > 0);
  try {
    await runMain(() => line("a"));
    assert.equal(process.exitCode, 0);
    await runMain(() =>
      Effect.gen(function* () {
        yield* line("b");
        yield* appFail("boom");
        yield* line("never");
      }),
    );
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
