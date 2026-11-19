// Fixtures and serial suites on Flick's runtime: when each fixture is made
// and released around the hooks of its suite, and which tests run at once.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Duration, sleep } from "@flick-lang/runtime/rt/duration";
import { lazyForce } from "@flick-lang/runtime/rt/effect/Lazy";
import { type Run, readOwnerEvidence } from "@flick-lang/runtime/rt/fiber";
import { fail, raise } from "@flick-lang/runtime/rt/cause";
import { addFinalizer, addFinalizerExit, within } from "@flick-lang/runtime/rt/owner";
import {
  runTests,
  testAfterAll,
  testAfterEach,
  testBeforeAll,
  testBeforeEach,
  testEffect,
  testFixture,
  testGroup,
  testSerial,
  testShare,
  testShared,
  testTimeout,
  testUse,
} from "@flick-lang/runtime/rt/testing";
import type { TestResult } from "@flick-lang/runtime/core/suites";

type Fixture = ReturnType<typeof testFixture<() => unknown>>;

const isResults = (v: unknown): v is { tests: ReadonlyArray<TestResult> } =>
  typeof v === "object" && v !== null && Array.isArray(Reflect.get(v, "tests"));

/** The name and status of each test of a run, and the cleanup of each that failed to clean up. */
const run = async (modules: Parameters<typeof runTests>[0], jobs = 1) => {
  let out = "";
  const before = process.exitCode;
  try {
    await runTests(modules, { reporter: "json", seed: 7, jobs, write: (text) => void (out += text) });
  } finally {
    process.exitCode = before;
  }
  const json: unknown = JSON.parse(out);
  assert.ok(isResults(json), out);
  return json.tests.map((t) => [t.name, t.status, t.cleanup]);
};

test("a fixture is made once for each test that uses it, a shared one once for its suite, and released after the hooks", async () => {
  const log: Array<string> = [];
  const note = (text: string) => () => void log.push(text);
  /** A fixture's initializer: it notes `name`, and `release name` when its scope closes. */
  const made = (name: string, from?: Fixture) =>
    function* (): Run {
      const base: unknown = from === undefined ? "" : yield lazyForce(testUse(from));
      log.push(`make ${name}${from === undefined ? "" : ` on ${String(base)}`}`);
      addFinalizer(note(`release ${name}`));
      return name;
    };
  const sees = (who: string, fixture: Fixture) =>
    function* (): Run {
      log.push(`${who} sees ${String(yield lazyForce(testUse(fixture)))}`);
      return undefined;
    };
  const db = testShared("M > s > db", made("db"));
  const conn = testFixture("M > s > conn", made("conn", db));
  const tx = testFixture("M > s > tx", made("tx"));
  const unused = testFixture("M > s > unused", made("unused"));
  const results = await run([
    [
      "M",
      [
        testSerial(
          testGroup("s", [
            testShare(db),
            testBeforeEach(note("before_each")),
            testAfterEach(sees("after_each", conn)),
            testAfterAll(sees("after_all", db)),
            testEffect("one", function* (): Run {
              yield lazyForce(testUse(conn));
              yield lazyForce(testUse(tx));
              // a fixture is made once for its test
              yield sees("one", conn)();
              return undefined;
            }),
            testEffect("two", note("two")),
          ]),
        ),
      ],
    ],
  ]);
  assert.deepEqual(results, [
    ["M > s > one", "passed", undefined],
    ["M > s > two", "passed", undefined],
  ]);
  assert.deepEqual(log, [
    "before_each",
    "make db",
    "make conn on db",
    "make tx",
    "one sees conn",
    "after_each sees conn",
    // the fixtures of a test are released after its after_each hooks, in reverse
    "release tx",
    "release conn",
    "before_each",
    "two",
    // a fixture the after_each hook uses first is made then
    "make conn on db",
    "after_each sees conn",
    "release conn",
    "after_all sees db",
    "release db",
  ]);
  // a fixture no code uses is never made
  assert.ok(!unused.shared && !log.includes("make unused"));
});

test("the scope of a test's fixtures closes with how the test ended", async () => {
  const log: Array<string> = [];
  const tx = testFixture("M > tx", () => {
    addFinalizerExit((exit) => void log.push(exit.cause === undefined ? "commit" : "rollback"));
    return "tx";
  });
  const using = (name: string, then: () => unknown) =>
    testEffect(name, function* (): Run {
      yield lazyForce(testUse(tx));
      return yield then();
    });
  const results = await run([
    [
      "M",
      [
        using("passes", () => undefined),
        using("fails", () => raise(fail("boom"))),
        testTimeout(10, using("times out", () => sleep(new Duration(3_600_000)))),
        testGroup("setup", [
          testBeforeEach(function* (): Run {
            yield lazyForce(testUse(tx));
            return raise(fail("no setup"));
          }),
          testEffect("never runs", () => undefined),
        ]),
      ],
    ],
  ]);
  assert.deepEqual(
    results.map(([name, status]) => [name, status]),
    [
      ["M > passes", "passed"],
      ["M > fails", "failed"],
      ["M > times out", "failed"],
      ["M > setup > never runs", "failed"],
    ],
  );
  assert.deepEqual(log, ["commit", "rollback", "rollback", "rollback"]);
});

test("the hooks of a suite run in its scope, so what they acquire is released with it", async () => {
  const log: Array<string> = [];
  const acquire = (name: string) => () => within(readOwnerEvidence(), () => addFinalizer(() => void log.push(`release ${name}`)));
  const results = await run([
    [
      "M",
      [
        testGroup("s", [
          testBeforeAll(acquire("before_all")),
          testAfterAll(acquire("after_all")),
          testEffect("a", () => void log.push("a")),
          testEffect("b", () => void log.push("b")),
        ]),
      ],
    ],
  ]);
  assert.deepEqual(results, [
    ["M > s > a", "passed", undefined],
    ["M > s > b", "passed", undefined],
  ]);
  // what `before_all` acquired is released after the suite's last test, before the `after_all` hooks run
  assert.deepEqual(log, ["a", "b", "release before_all", "release after_all"]);
});

test("the tests of a serial suite run one at a time, and the other tests beside them", async () => {
  const log: Array<string> = [];
  const timed = (name: string, millis = 20) =>
    testEffect(name, function* (): Run {
      log.push(`start ${name}`);
      yield sleep(new Duration(millis));
      log.push(`end ${name}`);
      return undefined;
    });
  const results = await run([["M", [testSerial(testGroup("serial", [timed("a"), timed("b")])), timed("c", 30)]]], 4);
  assert.deepEqual(
    results.map(([name, status]) => [name, status]),
    [
      ["M > serial > a", "passed"],
      ["M > serial > b", "passed"],
      ["M > c", "passed"],
    ],
  );
  assert.deepEqual(log, ["start a", "start c", "end a", "start b", "end c", "end b"]);
});
