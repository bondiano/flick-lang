// `runMain` on Flick's own runtime: the live `Host` services the launcher
// passes with `withHost` become the root of `main`, and what `main` provides
// itself wins.
import assert from "node:assert/strict";
import { test } from "node:test";

import { runMain } from "../src/rt/entry.ts";
import { operation, service } from "../src/rt/handle.ts";
import { withServices } from "../src/rt/layer.ts";
import { hostConsole, hostLiveServices, withHost } from "../src/rt/runner/host.ts";

/** `S.op arg` as compiled code performs it. */
const perform = (key: string, op: string, arg: unknown): unknown => {
  const f: unknown = operation(service(key), op, 1);
  if (typeof f !== "function") throw new TypeError("an operation is a function");
  const result: unknown = Reflect.apply(f, undefined, [arg]);
  return result;
};

/** Whether `S.op arg` finds an implementation. */
const provided = (key: string, op: string, arg: unknown): boolean => {
  try {
    perform(key, op, arg);
    return true;
  } catch {
    return false;
  }
};

test("main gets the live host services the launcher passes as its root", async (t) => {
  const written: Array<[string, string]> = [];
  t.mock.method(process.stdout, "write", (s: string) => written.push(["out", s]) > 0);
  t.mock.method(process.stderr, "write", (s: string) => written.push(["err", s]) > 0);
  process.env["FLICK_HOST_ROOT"] = "set";
  const seen: Array<unknown> = [];
  try {
    await runMain(withHost(hostLiveServices, () => {
      perform("Host.Console", "stdout_line", "from the root");
      perform("Host.Console", "stderr_line", "to stderr");
      perform("Host.Console", "stdout_write", "partial");
      seen.push(perform("Host.Environment", "env", "FLICK_HOST_ROOT"));
      seen.push(Array.isArray(perform("Host.Arguments", "args", undefined)));
      seen.push(typeof perform("Host.Clock", "current_time_millis", undefined));
      seen.push(typeof perform("Host.Clock", "monotonic_time_nanos", undefined));
      seen.push(typeof perform("Host.Random", "next", undefined));
    }));
    assert.equal(process.exitCode, 0);
  } finally {
    Reflect.deleteProperty(process.env, "FLICK_HOST_ROOT");
    process.exitCode = 0;
  }
  assert.deepEqual(written, [
    ["out", "from the root\n"],
    ["err", "to stderr\n"],
    ["out", "partial"],
  ]);
  assert.deepEqual(seen.slice(1), [true, "number", "bigint", "number"]);
  assert.equal(Reflect.get(Object(seen[0]), "_tag"), "Some");
});

test("a main that names only Host.Console gets no clock or random source", async (t) => {
  const written: Array<string> = [];
  t.mock.method(process.stdout, "write", (s: string) => written.push(s) > 0);
  const seen: Array<boolean> = [];
  try {
    await runMain(withHost([hostConsole], () => {
      seen.push(provided("Host.Console", "stdout_line", "console"));
      seen.push(provided("Host.Clock", "current_time_millis", undefined));
      seen.push(provided("Host.Random", "next", undefined));
    }));
    await runMain(() => {
      seen.push(provided("Host.Console", "stdout_line", "never"));
    });
  } finally {
    process.exitCode = 0;
  }
  assert.deepEqual([seen, written], [[true, false, false, false], ["console\n"]]);
});

test("what main provides itself wins over the root services", async (t) => {
  const written: Array<string> = [];
  t.mock.method(process.stdout, "write", (s: string) => written.push(s) > 0);
  const told: Array<string> = [];
  const fake = { stdout_line: (text: string) => void told.push(text) };
  try {
    await runMain(
      withHost(hostLiveServices, () =>
        withServices([[service("Host.Console"), fake]], () => perform("Host.Console", "stdout_line", "mine")),
      ),
    );
  } finally {
    process.exitCode = 0;
  }
  assert.deepEqual([told, written], [["mine"], []]);
});
