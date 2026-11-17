import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { test } from "node:test";

import { Effect, Option } from "effect";

import { devToolsListen } from "../src/effect/devtools/DevToolsServer.ts";
import {
  appFail,
  capabilities,
  hostConsole,
  provideHost,
  runMain,
  service,
  stdoutLine,
  versionProblems,
} from "../src/index.ts";

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
    let sent = false;
    child.stdout.on("data", (chunk: Buffer) => {
      stdout += chunk;
      // once: a second signal after the runtime removed its handlers would
      // kill the child with the default action
      if (signal && !sent && stdout.includes("ready\n")) {
        sent = true;
        child.kill(signal);
      }
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

test("main gets the live host services the launcher passes as its root", async () => {
  const r = await run("host-root", { env: { FLICK_HOST_ROOT: "set" } });
  assert.deepEqual(
    [r.code, r.stdout, r.stderr],
    [0, 'from the root\nset ["host-root"] number number\n', "to stderr\n"],
  );
});

test("what main provides itself wins over the root services", async () => {
  const r = await run("host-own");
  assert.deepEqual([r.code, r.stdout, r.stderr], [0, "fake: mine\n", ""]);
});

test("an explicit provide gives main its root services", async () => {
  const r = await run("host-provide");
  assert.deepEqual([r.code, r.stdout, r.stderr], [0, "fake: provided\n", ""]);
});

test("another Effect release is refused with exit code 2", async () => {
  const r = await run("old-effect");
  assert.deepEqual([r.code, r.stderr], [2, "error: the Flick Effect adapter requires effect 4.1.2, found 4.1.0\n"]);
});

test("the capability check names the version and missing operations", () => {
  assert.deepEqual(capabilities(), []);
  const problems = capabilities("4.1.0", { Effect: {} });
  assert.equal(problems[0], "the Flick Effect adapter requires effect 4.1.2, found 4.1.0");
  assert.ok(problems.includes("effect is missing Effect.gen"));
  assert.ok(problems.includes("effect is missing Scope.make"));
  assert.deepEqual(versionProblems(), []);
  assert.deepEqual(versionProblems("4.1.0"), [problems[0]]);
});

const line = (text: string) => Effect.sync(() => stdoutLine(text));

/** Whether the service `key` is provided. */
const named = (key: string) => Effect.map(Effect.serviceOption(service(key)), Option.isSome);

test("a main that names only Host.Console gets no clock or random source", async (t) => {
  t.mock.method(process.stdout, "write", () => true);
  const seen: Array<boolean> = [];
  const record = Effect.gen(function* () {
    for (const key of ["Host.Console", "Host.Clock", "Host.Random"]) seen.push(yield* named(key));
  });
  try {
    await runMain(() => record, { provide: provideHost([hostConsole]) });
    await runMain(() => record);
  } finally {
    process.exitCode = 0;
  }
  assert.deepEqual(seen, [true, false, false, false, false, false]);
});

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

test("runMain reports a defect by its stack and a typed failure by its JSON", async (t) => {
  const errors: Array<string> = [];
  t.mock.method(process.stderr, "write", (s: string) => errors.push(s) > 0);
  try {
    await runMain(() => Effect.die(new RangeError("Division by zero")));
    await runMain(() => Effect.fail({ _tag: "Refused#0", _0: "no" }));
    await runMain(() => Effect.die({ message: "plain" }));
  } finally {
    process.exitCode = 0;
  }
  const [defect, failure, plain] = errors;
  assert.match(defect ?? "", /^RangeError: Division by zero\n\s+at /);
  assert.doesNotMatch(defect ?? "", /Generator\.next/);
  assert.equal(failure, '{"_tag":"Refused#0","_0":"no"}\n');
  assert.equal(plain, "plain\n");
});

test("FLICK_DEVTOOLS sends the spans and the debug log to the DevTools", async () => {
  const server = await devToolsListen();
  try {
    const r = await run("devtools", { env: { FLICK_DEVTOOLS: `ws://127.0.0.1:${server.port}` } });
    assert.equal(r.code, 0);
    assert.match(r.stdout, /DEBUG .*atom App\.count\ndone\n$/);
    assert.deepEqual([server.spans, server.events], [["App.go"], ["atom App.count"]]);
  } finally {
    await server.close();
  }
});
