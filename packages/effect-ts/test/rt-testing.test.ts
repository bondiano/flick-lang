// The test runner on the kernel: outcomes reported apart, a seed that draws
// the same cases, a clock test on virtual time, and a suite built once.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Hash } from "effect";

import { fail, raise } from "@flick-lang/runtime/rt/cause";
import { clock } from "@flick-lang/runtime/rt/clock";
import { Duration, sleep } from "@flick-lang/runtime/rt/duration";
import { type Run, awaitFiber, currentCtx, fork } from "@flick-lang/runtime/rt/fiber";
import { hash } from "@flick-lang/runtime/rt/hash";
import { Layer } from "@flick-lang/runtime/rt/layer";
import { addFinalizer } from "@flick-lang/runtime/rt/owner";
import {
  genInt,
  runTests,
  testAdjust,
  testAfterAll,
  testAfterEach,
  testBeforeAll,
  testBeforeEach,
  testClock,
  testEffect,
  testGroup,
  testOnly,
  testProperty,
  testPure,
  testSkip,
  testSuite,
  testTimeout,
} from "@flick-lang/runtime/rt/testing";
import type { TestResult } from "@flick-lang/runtime/core/suites";
import { tag } from "@flick-lang/runtime/tags";

const PASS = Object.freeze({ _tag: tag("Test.Assertion.Pass") });
const failed = (message: string) => Object.freeze({ _tag: tag("Test.Assertion.Failed"), _0: message });
const assertionFailed = (message: string) => ({ _tag: tag("Test.AssertionFailed.AssertionFailed"), _0: message });

interface Report {
  readonly seed: number;
  readonly tests: ReadonlyArray<TestResult>;
}

const isReport = (v: unknown): v is Report => typeof v === "object" && v !== null && Array.isArray(Reflect.get(v, "tests"));

/** The JSON report of a run, and the exit code it set. */
const run = async (modules: Parameters<typeof runTests>[0], seed = 7, options: Parameters<typeof runTests>[1] = {}) => {
  let out = "";
  const before = process.exitCode;
  try {
    await runTests(modules, { ...options, reporter: "json", seed, write: (text) => void (out += text) });
    const json: unknown = JSON.parse(out);
    assert.ok(isReport(json), out);
    return { code: process.exitCode, json };
  } finally {
    process.exitCode = before;
  }
};

const HOUR = 3_600_000;

test("outcomes are reported apart", async () => {
  let builds = 0;
  let released = 0;
  const layer = new Layer(() => {
    builds++;
    addFinalizer(() => void released++);
    return [];
  });
  const { code, json } = await run([
    [
      "M",
      [
        testPure("adds", () => PASS),
        testPure("fails an assertion", () => failed("no")),
        testEffect("expects in a task", () => raise(fail(assertionFailed("expected")))),
        testEffect("fails with a typed error", () => raise(fail("boom"))),
        testEffect("dies", () => {
          throw new Error("crash");
        }),
        testEffect("fails to clean up", () =>
          addFinalizer(() => {
            throw new Error("release");
          }),
        ),
        testGroup("numbers", [testProperty("small", genInt(0, 100), (n) => (n < 50 ? PASS : failed("too big")))]),
        testClock("an hour on virtual time", function* (): Run {
          const sleeper = fork(function* (): Run {
            yield sleep(new Duration(HOUR));
            return clock().millis();
          }, currentCtx());
          yield testAdjust(new Duration(HOUR));
          const exit: unknown = yield awaitFiber(sleeper);
          return Reflect.get(Object(exit), "value") === HOUR ? undefined : raise(fail(assertionFailed("slept")));
        }),
        testSuite("suite", layer, [testEffect("one", () => undefined), testEffect("two", () => undefined)]),
      ],
    ],
  ]);
  assert.equal(code, 1);
  assert.equal(json.seed, 7);
  const status = Object.fromEntries(json.tests.map((t) => [t.name.replace("M > ", ""), [t.status, t.kind].join(" ").trim()]));
  assert.deepEqual(status, {
    adds: "passed",
    "fails an assertion": "failed assertion",
    "expects in a task": "failed assertion",
    "fails with a typed error": "failed failure",
    dies: "failed defect",
    "fails to clean up": "failed cleanup",
    "numbers > small": "failed assertion",
    "an hour on virtual time": "passed",
    "suite > one": "passed",
    "suite > two": "passed",
  });
  assert.deepEqual([builds, released], [1, 1]);
  // the same seed draws the same cases
  const again = await run([["M", [testGroup("numbers", [testProperty("small", genInt(0, 100), (n) => (n < 50 ? PASS : failed("too big")))])]]]);
  assert.equal(again.json.tests[0]?.message, json.tests.find((t) => t.name === "M > numbers > small")?.message);
});

test("hash is Effect's, so that a seed draws the same Gen.function", () => {
  const values = ["", 0, -0, 1.5, Number.NaN, 7n, true, "héllo😀", [1, "a"], { _tag: "X", _0: 1, _1: "b" }];
  assert.deepEqual(values.map(hash), values.map((v) => Hash.hash(v)));
});

test("a skipped test never runs, only keeps the run to its tests, and both select before a run", async () => {
  let ran: Array<string> = [];
  const effect = (name: string) => testEffect(name, () => void ran.push(name));
  const modules: Parameters<typeof runTests>[0] = [
    ["M", [effect("a"), testSkip(testGroup("later", [effect("b")])), testGroup("g", [effect("c")])]],
  ];
  const { code, json } = await run(modules);
  assert.equal(code, 0);
  assert.deepEqual(ran, ["a", "c"]);
  assert.deepEqual(json.tests, [
    { name: "M > a", status: "passed" },
    { name: "M > g > c", status: "passed" },
    { name: "M > later > b", status: "skipped" },
  ]);
  assert.equal(Reflect.get(json, "skipped"), 1);
  // `only` keeps the run to the marked tests; a skipped one stays skipped
  ran = [];
  const marked: Parameters<typeof runTests>[0] = [
    ["M", [effect("a"), testOnly(testGroup("g", [effect("c"), testSkip(effect("d"))]))]],
  ];
  const only = await run(marked);
  assert.deepEqual(ran, ["c"]);
  assert.deepEqual(
    only.json.tests.map((t) => [t.name, t.status]),
    [["M > g > c", "passed"], ["M > a", "skipped"], ["M > g > d", "skipped"]],
  );
  // `--list` prints the names a run would run and runs none; `--forbid-only` fails before running
  for (const [options, printed, exit] of [
    [{ list: true }, "M > g > c\n", 0],
    [{ forbidOnly: true }, "--forbid-only: 2 tests are marked only\n  M > g > c\n  M > g > d\n", 1],
  ] as const) {
    ran = [];
    let out = "";
    const before = process.exitCode;
    try {
      // oxlint-disable-next-line no-await-in-loop -- each run sets the process's one exit code
      await runTests(marked, { ...options, write: (text) => void (out += text) });
      assert.equal(process.exitCode, exit);
    } finally {
      process.exitCode = before;
    }
    assert.equal(out, printed);
    assert.deepEqual(ran, []);
  }
});

/** What a test of both levels runs, around its `body`. */
const each = (body: string) => ["before_each outer", "before_each inner", body, "after_each inner", "after_each outer"];

test("hooks run around the selected tests, outer to inner, and their cleanup in reverse", async () => {
  const log: Array<string> = [];
  const note = (text: string) => () => void log.push(text);
  const hooks = (level: string) => [
    testBeforeAll(note(`before_all ${level}`)),
    testBeforeEach(note(`before_each ${level}`)),
    testAfterEach(note(`after_each ${level}`)),
    testAfterAll(note(`after_all ${level}`)),
  ];
  const { code, json } = await run([
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
  assert.equal(code, 0);
  assert.deepEqual(json.tests.map((t) => [t.name, t.status]), [
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
  const note = (text: string) => () => void log.push(text);
  const { json } = await run([
    [
      "M",
      [
        testGroup("all", [
          testBeforeAll(() => raise(fail(assertionFailed("no database")))),
          testAfterAll(note("after_all")),
          testEffect("a", note("a")),
          testEffect("b", note("b")),
        ]),
        testGroup("each", [
          testAfterEach(note("after_each")),
          testBeforeEach(() => raise(fail("boom"))),
          testEffect("c", note("c")),
        ]),
      ],
    ],
  ]);
  assert.deepEqual(json.tests.map((t) => [t.name, t.status, t.kind, t.message]), [
    ["M > all > a", "failed", "setup", "no database"],
    ["M > all > b", "failed", "setup", "no database"],
    ["M > each > c", "failed", "setup", "fail boom"],
  ]);
  assert.deepEqual(log, ["after_all", "after_each"]);
});

test("a test out of time is interrupted, cleaned up and reported as such", async () => {
  const log: Array<string> = [];
  const slow = (name: string) =>
    testEffect(name, function* (): Run {
      yield sleep(new Duration(HOUR));
      log.push(`${name} woke`);
    });
  const modules: Parameters<typeof runTests>[0] = [
    [
      "M",
      [
        testGroup("g", [testAfterEach(() => void log.push("after_each")), testTimeout(20, slow("own")), slow("default")]),
        testTimeout(5000, testEffect("quick", () => undefined)),
      ],
    ],
  ];
  // `--timeout` gives the tests without a timeout of their own theirs
  const { json } = await run(modules, 7, { timeout: 30 });
  assert.deepEqual(json.tests.map((t) => [t.name, t.status, t.kind, t.message]), [
    ["M > g > own", "failed", "timeout", "timed out after 20 ms"],
    ["M > g > default", "failed", "timeout", "timed out after 30 ms"],
    ["M > quick", "passed", undefined, undefined],
  ]);
  assert.deepEqual(log, ["after_each", "after_each"]);
});

