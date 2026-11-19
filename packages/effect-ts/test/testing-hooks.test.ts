// The hooks of a group on Effect: their order around the selected tests, a
// failed setup, and a test out of time, each cleaned up as on the kernel.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect } from "effect";

import type { TestResult } from "@flick-lang/runtime/core/suites";
import { tag } from "@flick-lang/runtime/tags";

import {
  runTests,
  testAfterAll,
  testAfterEach,
  testBeforeAll,
  testBeforeEach,
  testEffect,
  testGroup,
  testPure,
  testSkip,
  testTimeout,
} from "../src/stdlib/testing.ts";

const PASS = Object.freeze({ _tag: tag("Test.Assertion.Pass") });
const assertionFailed = (message: string) => ({ _tag: tag("Test.AssertionFailed.AssertionFailed"), _0: message });

const isResults = (v: unknown): v is { tests: ReadonlyArray<TestResult> } =>
  typeof v === "object" && v !== null && Array.isArray(Reflect.get(v, "tests"));

/** The results of a run, each as its name, status, kind and message. */
const run = async (modules: Parameters<typeof runTests>[0], timeout?: number) => {
  let out = "";
  const before = process.exitCode;
  try {
    await runTests(modules, { reporter: "json", seed: 7, timeout, write: (text) => void (out += text) });
  } finally {
    process.exitCode = before;
  }
  const json: unknown = JSON.parse(out);
  assert.ok(isResults(json), out);
  return json.tests.map((t) => [t.name, t.status, t.kind, t.message]);
};

/** What a test of both levels runs, around its `body`. */
const each = (body: string) => ["before_each outer", "before_each inner", body, "after_each inner", "after_each outer"];

test("hooks run around the selected tests, outer to inner, and their cleanup in reverse", async () => {
  const log: Array<string> = [];
  const note = (text: string) => Effect.sync(() => void log.push(text));
  const hooks = (level: string) => [
    testBeforeAll(note(`before_all ${level}`)),
    testBeforeEach(note(`before_each ${level}`)),
    testAfterEach(note(`after_each ${level}`)),
    testAfterAll(note(`after_all ${level}`)),
  ];
  const results = await run([
    [
      "M",
      [
        testGroup("outer", [
          ...hooks("outer"),
          testGroup("inner", [...hooks("inner"), testEffect("one", note("one")), testEffect("two", note("two"))]),
          testPure("three", () => (log.push("three"), PASS)),
          // a group none of whose tests the run selects runs no hooks
          testSkip(testGroup("skipped", [...hooks("skipped"), testEffect("four", note("four"))])),
        ]),
      ],
    ],
  ]);
  assert.deepEqual(results.map(([name, status]) => [name, status]), [
    ["M > outer > inner > one", "passed"],
    ["M > outer > inner > two", "passed"],
    ["M > outer > three", "passed"],
    ["M > outer > skipped > four", "skipped"],
  ]);
  assert.deepEqual(log, [
    "before_all outer",
    "before_all inner",
    ...each("one"),
    ...each("two"),
    "after_all inner",
    "before_each outer",
    "three",
    "after_each outer",
    "after_all outer",
  ]);
});

test("a failed setup reports its tests as such and still runs what cleans up", async () => {
  const log: Array<string> = [];
  const note = (text: string) => Effect.sync(() => void log.push(text));
  const results = await run([
    [
      "M",
      [
        testGroup("all", [
          testBeforeAll(Effect.fail(assertionFailed("no database"))),
          testAfterAll(note("after_all")),
          testEffect("a", note("a")),
          testEffect("b", note("b")),
        ]),
        testGroup("each", [testAfterEach(note("after_each")), testBeforeEach(Effect.fail("boom")), testEffect("c", note("c"))]),
      ],
    ],
  ]);
  assert.deepEqual(results, [
    ["M > all > a", "failed", "setup", "no database"],
    ["M > all > b", "failed", "setup", "no database"],
    ["M > each > c", "failed", "setup", "fail boom"],
  ]);
  assert.deepEqual(log, ["after_all", "after_each"]);
});

test("a test out of time is interrupted, cleaned up and reported as such", async () => {
  const log: Array<string> = [];
  const slow = (name: string) => testEffect(name, Effect.andThen(Effect.sleep(3_600_000), Effect.sync(() => void log.push(`${name} woke`))));
  const results = await run(
    [
      [
        "M",
        [
          testGroup("g", [testAfterEach(Effect.sync(() => void log.push("after_each"))), testTimeout(20, slow("own")), slow("default")]),
          testTimeout(5000, testEffect("quick", Effect.void)),
        ],
      ],
    ],
    // `--timeout` gives the tests without a timeout of their own theirs
    30,
  );
  assert.deepEqual(results, [
    ["M > g > own", "failed", "timeout", "timed out after 20 ms"],
    ["M > g > default", "failed", "timeout", "timed out after 30 ms"],
    ["M > quick", "passed", undefined, undefined],
  ]);
  assert.deepEqual(log, ["after_each", "after_each"]);
});
