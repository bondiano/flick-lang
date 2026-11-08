// Tests on the kernel: what `Test` binds, and `runTests`, which runs each test
// in an owner of its own and reports assertions, typed failures, defects and
// failed cleanup apart, as the runner on Effect does.
import { type Engine, type Suites as SuitesOf, type TestOptions, type TestResult, parseTestArgs, runTestsWith } from "../core/test-run.ts";
import * as Core from "../core/testing.ts";
import { type Cause, type Exit, combine, failed, hasDies, hasInterruptsOnly, isFail, raise, succeeded } from "./cause.ts";
import { CLOCK, TestClock, currentTestClock } from "./clock.ts";
import { allOf } from "./concurrency.ts";
import { describeReport, report } from "./describe.ts";
import type { Duration } from "./duration.ts";
import { pretty } from "./entry.ts";
import { type Ctx, type Fiber, type Run, Under, asExit, awaitFiber, currentCtx, extend, fork } from "./fiber.ts";
import { type Layer, MemoMap, buildWith, servicesOf, withServices } from "./layer.ts";
import { Owner, OwnedExits, close, inside, ownedExits } from "./owner.ts";

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

/** A task: a function of nothing that runs when called. */
type Task = () => unknown;

type Test = Core.Test<Task, Layer>;
type Suite = Core.Suite<Task, Layer>;
type Named = Core.Named<Task, Layer>;

export const testEffect = (name: string, task: Task): Test => Object.freeze({ kind: "effect", name, task });

/** `Test.clock_test`: an effect test on a test clock of its own, made for each run. */
export const testClock = (name: string, task: Task): Test =>
  testEffect(name, () => new Under(extend(currentCtx(), CLOCK, new TestClock()), task));

/** `Test.adjust`: moves the test's clock forward. */
export const testAdjust = (d: Duration): Run =>
  currentTestClock().run(
    (now) => now + d.millis,
    Number.isFinite(d.millis) ? BigInt(Math.round(d.millis * 1e6)) : undefined,
  );

/** `Test.set_time`: sets the test's clock to `millis` since the epoch. */
export const testSetTime = (millis: number): Run => currentTestClock().run(() => millis, undefined);

export const testSuite = (name: string, layer: Layer, tests: ReadonlyArray<Test>): Test =>
  Object.freeze({ kind: "suite", name, layer, tests });

const describeCause = (cause: Cause): string => describeReport(report(cause));

/**
 * A suite in a run: an owner of its own, into which its layer is built by a
 * fiber of its own before its first selected test, and which closes after its
 * last one.
 */
interface SuiteRun {
  remaining: number;
  readonly layer: Layer;
  readonly owner: Owner;
  build: Fiber | undefined;
}

const suiteRuns = (selected: ReadonlyArray<Named>): Map<Suite, SuiteRun> => {
  const runs = new Map<Suite, SuiteRun>();
  for (const { suites } of selected) {
    for (const suite of suites) {
      const run = runs.get(suite);
      if (run === undefined) runs.set(suite, { remaining: 1, layer: suite.layer, owner: new Owner([]), build: undefined });
      else run.remaining++;
    }
  }
  return runs;
};

/** `task` with the services of the suites `runs`, outermost first, or the failure of their build. */
const withSuites = (task: Task, runs: ReadonlyArray<SuiteRun>, ctx: Ctx): Task =>
  runs.reduceRight<Task>(
    (body, run) =>
      function* (): Run {
        run.build ??= fork(() => inside(run.owner, () => buildWith(run.layer, new MemoMap(undefined), run.owner)), ctx);
        const built = asExit(yield awaitFiber(run.build));
        if (built.cause !== undefined) return raise(built.cause);
        return withServices(servicesOf(built.value), body);
      },
    task,
  );

type Problem = Readonly<{ kind: NonNullable<TestResult["kind"]>; message: string }>;

/** How a test body ended: `undefined` when it passed. */
const outcome = (exit: Exit): Problem | undefined => {
  const { cause } = exit;
  if (cause === undefined) {
    return typeof exit.value === "string" ? { kind: "assertion", message: exit.value } : undefined;
  }
  if (hasDies(cause)) return { kind: "defect", message: describeCause(cause) };
  if (hasInterruptsOnly(cause)) return { kind: "interrupted", message: "interrupted" };
  const fail = cause.reasons.find(isFail);
  if (fail && Core.isAssertionFailed(fail.error)) return { kind: "assertion", message: fail.error._0 };
  return { kind: "failure", message: describeCause(cause) };
};

/** The result of a test whose body ended with `exit` and its cleanup with `cleanup`. */
const resultOf = (name: string, { exit, cleanup, defects }: OwnedExits): TestResult => {
  // a supervisor that failed during the test fails it, as it fails a program
  const body = outcome(defects.reduce((out, cause) => failed(out.cause === undefined ? cause : combine(out.cause, cause)), exit));
  if (cleanup.cause === undefined) return body ? { name, status: "failed", ...body } : { name, status: "passed" };
  const why = describeCause(cleanup.cause);
  return { name, status: "failed", ...(body ?? { kind: "cleanup", message: why }), cleanup: why };
};

/** Runs one test, then closes the suites it ends, and gives its result to `done`. */
function* runOne({ name, test, suites }: Named, seed: number, runs: Map<Suite, SuiteRun>, ctx: Ctx, done: (r: TestResult) => void): Run {
  const mine = suites.flatMap((s) => runs.get(s) ?? []);
  const owned = yield ownedExits(() =>
    test.kind === "effect"
      ? (function* (): Run {
          yield withSuites(test.task, mine, ctx)();
          return undefined;
        })()
      : Core.checks(test, seed),
  );
  if (!(owned instanceof OwnedExits)) throw new TypeError("not the exits of a test");
  const r = resultOf(name, owned);
  let why: string | undefined;
  for (const run of mine.toReversed()) {
    run.remaining--;
    if (run.remaining > 0) continue;
    const closed = asExit(yield close(run.owner, succeeded(undefined)));
    if (closed.cause !== undefined) why = describeCause(closed.cause);
  }
  // the release of a suite fails the test that ended it
  done(why === undefined ? r : { ...r, status: "failed", ...(r.status === "passed" ? { kind: "cleanup", message: why } : {}), cleanup: why });
  return undefined;
}

/** The tests on the kernel: a fiber running them `jobs` at a time, whose interruption releases the suites. */
const engine: Engine<Task, Layer> = (selected, seed, jobs, hooks) => {
  const runs = suiteRuns(selected);
  const ctx = currentCtx();
  const results: Array<TestResult> = [];
  const tasks = selected.map((t, index) => () => {
    hooks?.started(index, t.name);
    return runOne(t, seed, runs, ctx, (r) => {
      results[index] = r;
      hooks?.finished(index, r);
    });
  });
  const fiber = fork(() => allOf(tasks, jobs), ctx);
  const ended = (async () => {
    const exit = await new Promise<Exit>((resolve) => fiber.observe(resolve));
    // an interrupted run still releases what its suites made
    const open = [...runs.values()].filter((r) => r.remaining > 0);
    await Promise.all(
      open.map((r) => new Promise((resolve) => fork(() => close(r.owner, succeeded(undefined)), ctx).observe(resolve))),
    );
    return exit.cause === undefined ? results : { failure: pretty(exit.cause) };
  })();
  return { ended, interrupt: () => fiber.interrupt(-1) };
};

/** The `tests` of each test module, by module name. */
export type Suites = SuitesOf<Task, Layer>;

/** Runs the `tests` of each module on the kernel, and sets the exit code (`runTestsWith`). */
export const runTests = (modules: Suites, options: TestOptions = parseTestArgs(process.argv.slice(2))): Promise<void> =>
  runTestsWith(modules, options, engine);
