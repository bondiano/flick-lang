// Tests on Effect. A test is data built by pure code; `runTests` runs them,
// each in an owner of its own, and reports assertions, typed failures,
// defects and failed cleanup apart.
import { Cause, type Context, type Duration, Effect, Exit, Fiber, Layer, Scope } from "effect";
import * as TestClock from "effect/testing/TestClock";

import { describeCause } from "../cause.ts";
import { type Engine, type Suites as SuitesOf, type TestOptions, type TestResult, parseTestArgs, runTestsWith } from "../core/test-run.ts";
import * as Core from "../core/testing.ts";
import { service } from "../effects.ts";
import { buildInto } from "../layer.ts";
import { Owner, close, inside, ownedExits } from "../owner.ts";

export {
  genBool,
  genConstant,
  genFunction,
  genInt,
  genList,
  genMap,
  genNumber,
  genOneOf,
  genPair,
  genString,
  testGroup,
  testProperty,
  testPure,
} from "../core/testing.ts";
export type { Assertion, Gen, Rng } from "../core/testing.ts";
export { parseTestArgs } from "../core/test-run.ts";
export type { FailureKind, TestOptions, TestResult } from "../core/test-run.ts";

type Task = Effect.Effect<unknown, unknown>;
type AnyLayer = Layer.Layer<unknown, unknown>;

/** `Test.Test`. */
export type Test = Core.Test<Task, AnyLayer>;
type Suite = Core.Suite<Task, AnyLayer>;
type Leaf = Core.Leaf<Task, AnyLayer>;
type Named = Core.Named<Task, AnyLayer>;

export const testEffect = (name: string, task: Task): Test => Object.freeze({ kind: "effect", name, task });

/** The service `Test.VirtualTime`: a test that may move its clock. */
const VirtualTime = /*#__PURE__*/ service("Test.VirtualTime");

/** `Test.clock_test`: an effect test on a TestClock of its own, made for each run. */
export const testClock = (name: string, task: Effect.Effect<unknown, unknown, unknown>): Test =>
  testEffect(
    name,
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides every service the row names
    Effect.provide(task, Layer.merge(TestClock.layer(), Layer.succeed(VirtualTime, {}))) as Task,
  );

/** `Test.adjust`: moves the test's clock forward. */
export const testAdjust = (duration: Duration.Duration): Effect.Effect<void> => TestClock.adjust(duration);

/** `Test.set_time`: sets the test's clock to `millis` since the epoch. */
export const testSetTime = (millis: number): Effect.Effect<void> => TestClock.setTime(millis);

export const testSuite = (name: string, layer: AnyLayer, tests: ReadonlyArray<Test>): Test =>
  Object.freeze({ kind: "suite", name, layer, tests });

/**
 * A suite in a run: an owner of its own, into which its layer is built
 * before its first selected test, and which closes after its last one.
 */
interface SuiteRun {
  remaining: number;
  readonly owner: Owner;
  /** The build, run once, its outcome shared by the suite's tests. */
  readonly services: Effect.Effect<Exit.Exit<Context.Context<unknown>, unknown>>;
}

const suiteRuns = (selected: ReadonlyArray<Named>): Map<Suite, SuiteRun> => {
  const runs = new Map<Suite, SuiteRun>();
  for (const { suites } of selected) {
    for (const suite of suites) {
      const run = runs.get(suite);
      if (run !== undefined) {
        run.remaining++;
        continue;
      }
      const owner = new Owner(Effect.runSync(Scope.make()), []);
      const build = Effect.exit(inside(owner, buildInto(suite.layer, owner.resources)));
      runs.set(suite, { remaining: 1, owner, services: Effect.runSync(Effect.cached(build)) });
    }
  }
  return runs;
};

/** The test's task with the services of its suites, or the failure of their build. */
const withSuites = (task: Task, runs: ReadonlyArray<SuiteRun>): Task =>
  runs.reduceRight<Task>(
    (body, run) =>
      Effect.flatMap(run.services, (built) =>
        Exit.isSuccess(built) ? Effect.provideContext(body, built.value) : Effect.failCause(built.cause),
      ),
    task,
  );

/** After a test: the suites it ended, closed; why one failed to close. */
const leave = (runs: ReadonlyArray<SuiteRun>): Effect.Effect<string | undefined> =>
  Effect.gen(function* () {
    let failed: string | undefined;
    for (const run of runs.toReversed()) {
      run.remaining--;
      if (run.remaining > 0) continue;
      const closed = yield* close(run.owner, Exit.void);
      if (Exit.isFailure(closed)) failed = describeCause(closed.cause);
    }
    return failed;
  });

/** The checks of a pure or property test, as an effect whose throw is a defect. */
const checks = (test: Exclude<Leaf, { kind: "effect" }>, seed: number): Effect.Effect<string | undefined> =>
  Effect.sync(() => Core.checks(test, seed));

type Problem = Readonly<{ kind: NonNullable<TestResult["kind"]>; message: string }>;

/** How a test body ended: `undefined` when it passed. */
const outcome = (exit: Exit.Exit<string | undefined, unknown>): Problem | undefined => {
  if (Exit.isSuccess(exit)) {
    return exit.value === undefined ? undefined : { kind: "assertion", message: exit.value };
  }
  const { cause } = exit;
  if (Cause.hasDies(cause)) return { kind: "defect", message: describeCause(cause) };
  if (Cause.hasInterruptsOnly(cause)) return { kind: "interrupted", message: "interrupted" };
  const fail = cause.reasons.find(Cause.isFailReason);
  if (fail && Core.isAssertionFailed(fail.error)) return { kind: "assertion", message: fail.error._0 };
  return { kind: "failure", message: describeCause(cause) };
};

const runOne = ({ name, test, suites }: Named, seed: number, runs: Map<Suite, SuiteRun>): Effect.Effect<TestResult> => {
  const mine = suites.flatMap((s) => runs.get(s) ?? []);
  const result = runLeaf(name, test, seed, mine);
  if (mine.length === 0) return result;
  return Effect.flatMap(result, (r) =>
    Effect.map(leave(mine), (failed): TestResult => {
      if (failed === undefined) return r;
      // the suite's release fails the test that ended it
      return { ...r, status: "failed", ...(r.status === "passed" ? { kind: "cleanup", message: failed } : {}), cleanup: failed };
    }),
  );
};

const runLeaf = (name: string, test: Leaf, seed: number, suites: ReadonlyArray<SuiteRun>): Effect.Effect<TestResult> =>
  Effect.map(
    ownedExits(() =>
      test.kind === "effect" ? Effect.as(withSuites(test.task, suites), undefined) : checks(test, seed),
    ),
    ({ exit, cleanup, defects }): TestResult => {
      // a supervisor that failed during the test fails it, as it fails a program
      const body = outcome(
        defects.reduce<typeof exit>(
          (out, cause) => Exit.failCause(Exit.isFailure(out) ? Cause.combine(out.cause, cause) : cause),
          exit,
        ),
      );
      if (Exit.isSuccess(cleanup)) return body ? { name, status: "failed", ...body } : { name, status: "passed" };
      const failed = describeCause(cleanup.cause);
      return { name, status: "failed", ...(body ?? { kind: "cleanup", message: failed }), cleanup: failed };
    },
  );

/** The tests on Effect: a fiber of `Effect.forEach`, whose interruption releases the suites. */
const engine: Engine<Task, AnyLayer> = (selected, seed, jobs, hooks) => {
  const runs = suiteRuns(selected);
  const fiber = Effect.runFork(
    Effect.forEach(
      selected,
      (t, index) =>
        hooks
          ? Effect.suspend(() => {
              hooks.started(index, t.name);
              return Effect.tap(runOne(t, seed, runs), (result) => Effect.sync(() => hooks.finished(index, result)));
            })
          : runOne(t, seed, runs),
      { concurrency: jobs },
    ),
  );
  const ended = (async () => {
    const exit = await new Promise<Exit.Exit<Array<TestResult>>>((resolve) => fiber.addObserver(resolve));
    // an interrupted run still releases what its suites made
    await Promise.all([...runs.values()].filter((r) => r.remaining > 0).map((r) => Effect.runPromise(close(r.owner, Exit.void))));
    return Exit.isSuccess(exit) ? exit.value : { failure: Cause.pretty(exit.cause) };
  })();
  return { ended, interrupt: () => void Effect.runFork(Fiber.interrupt(fiber)) };
};

/** The `tests` of each test module, by module name. */
export type Suites = SuitesOf<Task, AnyLayer>;

/** Runs the `tests` of each module on Effect, and sets the exit code (`runTestsWith`). */
export const runTests = (modules: Suites, options: TestOptions = parseTestArgs(process.argv.slice(2))): Promise<void> =>
  runTestsWith(modules, options, engine);
