// The test runner on the kernel: outcomes reported apart, a seed that draws
// the same cases, a clock test on virtual time, and a suite built once.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Hash } from "effect";

import { fail, raise } from "../src/rt/cause.ts";
import { clock } from "../src/rt/clock.ts";
import { Duration, sleep } from "../src/rt/duration.ts";
import { type Run, awaitFiber, currentCtx, fork } from "../src/rt/fiber.ts";
import { hash } from "../src/rt/hash.ts";
import { Layer } from "../src/rt/layer.ts";
import { addFinalizer } from "../src/rt/owner.ts";
import {
  genInt,
  runTests,
  testAdjust,
  testClock,
  testEffect,
  testGroup,
  testProperty,
  testPure,
  testSuite,
} from "../src/rt/testing.ts";
import type { TestResult } from "../src/core/suites.ts";
import { tag } from "../src/tags.ts";

const PASS = Object.freeze({ _tag: tag("Test.Assertion.Pass") });
const failed = (message: string) => Object.freeze({ _tag: tag("Test.Assertion.Failed"), _0: message });
const assertionFailed = (message: string) => ({ _tag: tag("Test.AssertionFailed.AssertionFailed"), _0: message });

interface Report {
  readonly seed: number;
  readonly tests: ReadonlyArray<TestResult>;
}

const isReport = (v: unknown): v is Report => typeof v === "object" && v !== null && Array.isArray(Reflect.get(v, "tests"));

/** The JSON report of a run, and the exit code it set. */
const run = async (modules: Parameters<typeof runTests>[0], seed = 7) => {
  let out = "";
  const before = process.exitCode;
  try {
    await runTests(modules, { reporter: "json", seed, write: (text) => void (out += text) });
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
