// Tests on the kernel: what `Test` binds, and `runTests`, which runs each test
// in an owner of its own and reports assertions, typed failures, defects and
// failed cleanup apart, as the runner on Effect does.
import { type Engine, type Suites as SuitesOf, type TestOptions, type TestResult, parseTestArgs, runTestsWith } from "../core/suites.ts";
import * as Core from "../core/testing.ts";
import { type Cause, type Exit, combine, failed, hasDies, hasInterruptsOnly, isFail, raise, succeeded } from "./cause.ts";
import { CLOCK, TestClock, currentTestClock } from "./clock.ts";
import { allOf, timeoutOption } from "./concurrency.ts";
import { describeReport, report } from "./describe.ts";
import type { Duration } from "./duration.ts";
import { pretty } from "./entry.ts";
import { type Ctx, type Fiber, type Run, Under, asExit, awaitFiber, currentCtx, exitOf, extend, fork } from "./fiber.ts";
import { type Layer, MemoMap, type Services, buildWith, servicesOf, withServices } from "./layer.ts";
import { Owner, OwnedExits, addFinalizer, close, inside, ownedExits } from "./owner.ts";
import { hostLiveServices } from "./runner/host.ts";

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
  testOnly,
  testProperty,
  testPure,
  testSkip,
  testTimeout,
  testBeforeAll,
  testBeforeEach,
  testAfterEach,
  testAfterAll,
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

/** A `before_*` hook that failed with `cause`: the setup of a test. */
class Setup {
  readonly cause: Cause;
  constructor(cause: Cause) {
    this.cause = cause;
  }
}

/** A test whose `before_each` hooks and body ran out of their `millis`. */
class TimedOut {
  readonly millis: number;
  constructor(millis: number) {
    this.millis = millis;
  }
}

/**
 * A suite in a run: an owner of its own, set up by a fiber of its own before
 * its first selected test, and which closes after its last one.
 */
interface SuiteRun {
  remaining: number;
  readonly suite: Suite;
  readonly owner: Owner;
  build: Fiber | undefined;
}

const suiteRuns = (selected: ReadonlyArray<Named>): Map<Suite, SuiteRun> => {
  const runs = new Map<Suite, SuiteRun>();
  for (const { suites } of selected) {
    for (const suite of suites) {
      const run = runs.get(suite);
      if (run === undefined) runs.set(suite, { remaining: 1, suite, owner: new Owner([]), build: undefined });
      else run.remaining++;
    }
  }
  return runs;
};

/** The hooks `tasks` run in order: a `Setup` of the first that fails. */
function* runHooks(tasks: ReadonlyArray<Task>): Run {
  for (const task of tasks) {
    const exit = asExit(yield exitOf(task));
    // an interrupted hook interrupts the test, a failed one fails its setup
    if (exit.cause !== undefined) return hasInterruptsOnly(exit.cause) ? raise(exit.cause) : new Setup(exit.cause);
  }
  return undefined;
}

/**
 * The setup of a suite, in its owner: its `after_all` hooks are finalizers,
 * whatever its `before_all` hooks do, then those run, then its layer is built.
 */
function* setUp({ suite: { hooks: own, layer }, owner }: SuiteRun): Run {
  for (const task of own?.after_all ?? []) addFinalizer(task);
  const setup = yield runHooks(own?.before_all ?? []);
  if (setup instanceof Setup || layer === undefined) return setup;
  return yield buildWith(layer, new MemoMap(undefined), owner);
}

/**
 * A test in its scope: the suites it is in set up, outermost first, then,
 * within its `timeout`, the `before_each` hooks of each, outermost first,
 * and `body` with their services; the `after_each` hooks are finalizers of
 * the test's scope.
 */
function* runCase(body: Task, runs: ReadonlyArray<SuiteRun>, timeout: number | undefined, ctx: Ctx): Run {
  const services: Array<Services> = [];
  for (const run of runs) {
    run.build ??= fork(() => inside(run.owner, () => setUp(run)), ctx);
    const built = asExit(yield awaitFiber(run.build));
    if (built.cause !== undefined) return raise(built.cause);
    if (built.value instanceof Setup) return built.value;
    if (run.suite.layer !== undefined) services.push(servicesOf(built.value));
  }
  function* each(): Run {
    for (const { suite } of runs) {
      for (const task of suite.hooks?.after_each ?? []) addFinalizer(task);
      const setup = yield runHooks(suite.hooks?.before_each ?? []);
      if (setup instanceof Setup) return setup;
    }
    return yield services.reduceRight<Task>((inner, s) => () => withServices(s, inner), body)();
  }
  if (timeout === undefined) return yield each();
  const done: unknown = yield timeoutOption(timeout, each);
  // `Some` of the test's value when it ended in time
  if (typeof done !== "object" || done === null || Reflect.get(done, "_tag") !== "Some") return new TimedOut(timeout);
  const value: unknown = Reflect.get(done, "value");
  return value;
}

type Problem = Readonly<{ kind: NonNullable<TestResult["kind"]>; message: string }>;

/** How a test body ended: `undefined` when it passed. */
const outcome = (exit: Exit): Problem | undefined => {
  const { cause } = exit;
  if (cause === undefined) {
    const { value } = exit;
    if (value instanceof Setup) return { kind: "setup", message: outcome(failed(value.cause))?.message ?? "" };
    if (value instanceof TimedOut) return { kind: "timeout", message: `timed out after ${value.millis} ms` };
    return typeof value === "string" ? { kind: "assertion", message: value } : undefined;
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
function* runOne({ name, test, suites, timeout }: Named, seed: number, runs: Map<Suite, SuiteRun>, ctx: Ctx, done: (r: TestResult) => void): Run {
  const mine = suites.flatMap((s) => runs.get(s) ?? []);
  const body =
    test.kind === "effect"
      ? function* (): Run {
          yield test.task();
          return undefined;
        }
      : () => Core.checks(test, seed);
  const owned = yield ownedExits(() => runCase(body, mine, timeout, ctx));
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
  // the tests and their suites run with the live host services, as a `main` of `Runner.Node`
  const ctx = hostLiveServices.reduce((c, [key, impl]) => extend(c, key, impl), currentCtx());
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
