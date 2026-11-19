// Fixtures and serial suites on Effect: when each fixture is made and
// released around the hooks of its suite, and which tests run at once, as
// on the kernel.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Option } from "effect";

import type { TestResult } from "@flick-lang/runtime/core/suites";

import { lazyForce } from "../src/effect/Lazy.ts";
import { flickAddFinalizer, flickAddFinalizerExit } from "../src/effect/Scope.ts";
import { within } from "../src/owner.ts";
import { readOwnerEvidence } from "../src/owner_evidence.ts";
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
} from "../src/stdlib/testing.ts";

type Task = Effect.Effect<unknown, unknown>;
type Fixture = ReturnType<typeof testFixture<Task>>;

const isResults = (v: unknown): v is { tests: ReadonlyArray<TestResult> } =>
  typeof v === "object" && v !== null && Array.isArray(Reflect.get(v, "tests"));

/** The name, status and cleanup of each test of a run. */
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

/** `finalizer`, added to the scope of the code's own region, as generated code adds one. */
const release = (finalizer: Effect.Effect<void>): Task => Effect.flatMap(readOwnerEvidence(), (owner) => within(owner, flickAddFinalizer(finalizer)));

/** `await fixture`. */
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the initializers here need no services
const force = (fixture: Fixture): Task => Effect.flatMap(testUse(fixture), lazyForce) as Task;

test("a fixture is made once for each test that uses it, a shared one once for its suite, and released after the hooks", async () => {
  const log: Array<string> = [];
  const note = (text: string) => Effect.sync(() => void log.push(text));
  /** A fixture's initializer: it notes `name`, and `release name` when its scope closes. */
  const made = (name: string, from?: Fixture): Task =>
    Effect.gen(function* () {
      const base = from === undefined ? "" : ` on ${String(yield* force(from))}`;
      yield* note(`make ${name}${base}`);
      yield* release(note(`release ${name}`));
      return name;
    });
  const sees = (who: string, fixture: Fixture): Task => Effect.flatMap(force(fixture), (value) => note(`${who} sees ${String(value)}`));
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
            // a fixture is made once for its test
            testEffect("one", Effect.andThen(Effect.andThen(force(conn), force(tx)), sees("one", conn))),
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
  const ended = flickAddFinalizerExit((cause) => Effect.sync(() => void log.push(Option.isNone(cause) ? "commit" : "rollback")));
  const tx = testFixture<Task>("M > tx", Effect.flatMap(readOwnerEvidence(), (owner) => within(owner, Effect.as(ended, "tx"))));
  const using = (name: string, then: Task) => testEffect(name, Effect.andThen(force(tx), then));
  const results = await run([
    [
      "M",
      [
        using("passes", Effect.void),
        using("fails", Effect.fail("boom")),
        testTimeout(10, using("times out", Effect.never)),
        testGroup("setup", [testBeforeEach(Effect.andThen(force(tx), Effect.fail("no setup"))), testEffect("never runs", Effect.void)]),
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
  const acquire = (name: string) => release(Effect.sync(() => void log.push(`release ${name}`)));
  const results = await run([
    [
      "M",
      [
        testGroup("s", [
          testBeforeAll(acquire("before_all")),
          testAfterAll(acquire("after_all")),
          testEffect("a", Effect.sync(() => void log.push("a"))),
          testEffect("b", Effect.sync(() => void log.push("b"))),
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
    testEffect(
      name,
      Effect.gen(function* () {
        log.push(`start ${name}`);
        yield* Effect.sleep(millis);
        log.push(`end ${name}`);
      }),
    );
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
