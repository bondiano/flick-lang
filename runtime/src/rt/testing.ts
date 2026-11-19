// Tests on the kernel: what `Test` binds, and `runTests`, which runs each test
// in an owner of its own and reports assertions, typed failures, defects and
// failed cleanup apart, as the runner on Effect does.
import { type Engine, type Suites as SuitesOf, type TestOptions, type TestResult, parseTestArgs, runTestsWith } from "../core/suites.ts";
import { type Option, none, some } from "../core/data.ts";
import type { SnapshotCase } from "../core/snapshots.ts";
import * as Core from "../core/testing.ts";
import { type Cause, type Exit, combine, fail, failed, hasDies, hasInterruptsOnly, interrupt, isFail, raise, succeeded } from "./cause.ts";
import { CLOCK, TestClock, currentTestClock } from "./clock.ts";
import { allOf, timeoutOption } from "./concurrency.ts";
import { describeReport, report } from "./describe.ts";
import type { Duration } from "./duration.ts";
import { pretty } from "./entry.ts";
import { type LazyCell, lazyMake } from "./effect/Lazy.ts";
import { type Ctx, type Fiber, type Run, Under, asExit, awaitFiber, currentCtx, exitOf, extend, fork, lookup, withOwnerEvidence } from "./fiber.ts";
import { type Layer, MemoMap, type Services, buildWith, servicesOf, withServices } from "./layer.ts";
import { Owner, OwnedExits, addFinalizer, close, closeOwn, inside, ownedExits } from "./owner.ts";
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
  testFixture,
  testShared,
  testShare,
} from "../core/testing.ts";

/** A task: a function of nothing that runs when called. */
type Task = () => unknown;

type Test = Core.Test<Task, Layer>;
type Suite = Core.Suite<Task, Layer>;
type Named = Core.Named<Task, Layer>;

export const testEffect = (name: string, task: Task): Test => Object.freeze({ kind: "effect", name, task });

/** `Test.before_all` and the other hooks of the group they are in. */
export const testBeforeAll = (task: Task): Test => Core.testBeforeAll(task);
export const testBeforeEach = (task: Task): Test => Core.testBeforeEach(task);
export const testAfterEach = (task: Task): Test => Core.testAfterEach(task);
export const testAfterAll = (task: Task): Test => Core.testAfterAll(task);

/** `Test.timeout`: the test, and each test in it without a timeout of its own, given `millis`. */
export const testTimeout = (millis: number, test: Test): Test => Core.testTimeout(millis, test);

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

/** `Test.serial`: the test, and each test in it, run one at a time. */
export const testSerial = (test: Test): Test => Core.testSerial(test);

export const testSuite = (name: string, layer: Layer, tests: ReadonlyArray<Test>): Test =>
  Object.freeze({ kind: "suite", name, layer, tests });

const describeCause = (cause: Cause): string => describeReport(report(cause));

/** A `before_*` hook that failed with `cause`, or ran out of time as `cause` says: the setup of a test. */
class Setup {
  readonly cause: Cause | string;
  constructor(cause: Cause | string) {
    this.cause = cause;
  }
}

/** Whether `value` is `Some` of an `Option`, as `timeout_option` gives. */
const isSome = (value: unknown): value is Readonly<{ value: unknown }> =>
  typeof value === "object" && value !== null && Reflect.get(value, "_tag") === "Some";

const timedOut = (what: string, millis: number): string => `${what} timed out after ${millis} ms`;

/**
 * The hook `task`, within `millis` when there are some: one out of time is
 * interrupted and fails with what timed out, the cleanup failure of a
 * finalizer.
 */
const bounded = (task: Task, what: string, millis: number | undefined): Task =>
  millis === undefined
    ? task
    : function* (): Run {
        const done: unknown = yield timeoutOption(millis, task);
        return isSome(done) ? undefined : raise(fail(timedOut(what, millis)));
      };

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
  /** Its shared fixtures, each made the first time code uses it. */
  readonly shared: Cells;
}

type Fixture = Core.Fixture<Task>;

/**
 * The fixtures of a test or a suite's run, and the owner they are made in:
 * a scope of their own, which the scope of the test or the suite closes
 * after the hooks that run when it closes, as those may use them.
 */
type Cells = Readonly<{ cells: Map<Fixture, LazyCell>; owner: Owner }>;

const cellsIn = (outer: Owner): Cells => ({ cells: new Map(), owner: new Owner(outer.defects) });

/** The finalizer that closes `cells` with `outcome`: it fails as their finalizers do. */
const closing = ({ owner }: Cells, outcome: () => Exit = () => succeeded(undefined)) =>
  function* (): Run {
    const ended = asExit(yield closeOwn(owner, outcome()));
    return ended.cause === undefined ? undefined : raise(ended.cause);
  };

/**
 * What code finds its fixtures in: those of the test it runs for, if any,
 * and the suites around it, outermost first, whose shared fixtures it sees.
 */
class Fixtures {
  readonly test: Cells | undefined;
  readonly suites: ReadonlyArray<SuiteRun>;
  constructor(test: Fixtures["test"], suites: ReadonlyArray<SuiteRun>) {
    this.test = test;
    this.suites = suites;
  }
}

const FIXTURES: object = /*#__PURE__*/ Object.freeze({ key: "Test.fixtures" });

/** `task`, with `fixtures` for the fixtures it uses. */
const withFixtures =
  (fixtures: Fixtures, task: Task): Task =>
  () =>
    new Under(extend(currentCtx(), FIXTURES, fixtures), task);

/** The cell of `fixture`, made the first time: its initializer runs in the scope of the cells. */
const cellOf = ({ cells, owner }: Cells, fixture: Fixture): LazyCell => {
  const found = cells.get(fixture);
  if (found !== undefined) return found;
  const cell = lazyMake(() => inside(owner, () => withOwnerEvidence(owner, fixture.init)));
  cells.set(fixture, cell);
  return cell;
};

/**
 * `Test.use`: the `Lazy` of `fixture` for the test this code runs for, or
 * for the run of the innermost suite around it that shares it.
 */
export const testUse = (fixture: Fixture): LazyCell => {
  const fixtures = lookup(FIXTURES);
  if (!(fixtures instanceof Fixtures)) throw new Error(`the fixture ${fixture.key} runs only in the code of a test or a suite that \`flick test\` runs`);
  if (fixture.shared) {
    const run = fixtures.suites.findLast((r) => r.suite.shares?.has(fixture) === true);
    if (run === undefined) throw new Error(`the shared fixture ${fixture.key}, used by code that no suite sharing it runs`);
    return cellOf(run.shared, fixture);
  }
  if (fixtures.test === undefined) throw new Error(`the fixture ${fixture.key} of a test, used by code that runs for no test`);
  return cellOf(fixtures.test, fixture);
};

const suiteRuns = (selected: ReadonlyArray<Named>): Map<Suite, SuiteRun> => {
  const runs = new Map<Suite, SuiteRun>();
  for (const { suites } of selected) {
    for (const suite of suites) {
      const run = runs.get(suite);
      if (run !== undefined) {
        run.remaining++;
        continue;
      }
      const owner = new Owner([]);
      runs.set(suite, { remaining: 1, suite, owner, build: undefined, shared: cellsIn(owner) });
    }
  }
  return runs;
};

/** The hooks `tasks` run in order, each within `millis` if any: a `Setup` of the first that fails. */
function* runHooks(tasks: ReadonlyArray<Task>, what: string, millis?: number): Run {
  for (const task of tasks) {
    const exit = asExit(yield exitOf(millis === undefined ? task : () => timeoutOption(millis, task)));
    // an interrupted hook interrupts the test, a failed one fails its setup
    if (exit.cause !== undefined) return hasInterruptsOnly(exit.cause) ? raise(exit.cause) : new Setup(exit.cause);
    if (millis !== undefined && !isSome(exit.value)) return new Setup(timedOut(what, millis));
  }
  return undefined;
}

/**
 * The setup of a suite, in its owner: its `after_all` hooks are finalizers,
 * whatever its `before_all` hooks do, then those run, then its layer is
 * built; its hooks have its timeout, if it has one.
 */
function* setUp({ suite: { hooks: own, layer, timeout }, owner, shared }: SuiteRun, suites: ReadonlyArray<SuiteRun>): Run {
  // its shared fixtures close after its `after_all` hooks, which may use them
  addFinalizer(closing(shared));
  // the hooks of a suite see its shared fixtures and those of the suites around it
  const fixtures = new Fixtures(undefined, suites);
  for (const task of own?.after_all ?? []) addFinalizer(bounded(withFixtures(fixtures, task), "after_all", timeout));
  const setup = yield runHooks((own?.before_all ?? []).map((task) => withFixtures(fixtures, task)), "before_all", timeout);
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
  for (const [level, run] of runs.entries()) {
    const around = runs.slice(0, level + 1);
    // the hooks of a suite run in its scope, which their own scopes are in
    run.build ??= fork(() => inside(run.owner, () => withOwnerEvidence(run.owner, () => setUp(run, around))), ctx);
    const built = asExit(yield awaitFiber(run.build));
    if (built.cause !== undefined) return raise(built.cause);
    if (built.value instanceof Setup) return built.value;
    if (run.suite.layer !== undefined) services.push(servicesOf(built.value));
  }
  // the fixtures of the test, made the first time its code uses each
  const test = cellsIn(currentCtx().owner ?? new Owner([]));
  // how the case ended, which their scope closes with: what it owns sees whether the case passed
  let outcome: Exit = succeeded(undefined);
  addFinalizer(closing(test, () => outcome));
  function* each(): Run {
    for (const [level, { suite }] of runs.entries()) {
      const fixtures = new Fixtures(test, runs.slice(0, level + 1));
      for (const task of suite.hooks?.after_each ?? []) addFinalizer(bounded(withFixtures(fixtures, task), "after_each", suite.timeout));
      // the test's timeout bounds its `before_each` hooks and its body together
      const setup = yield runHooks((suite.hooks?.before_each ?? []).map((task) => withFixtures(fixtures, task)), "before_each");
      if (setup instanceof Setup) return setup;
    }
    const own = withFixtures(new Fixtures(test, runs), body);
    return yield services.reduceRight<Task>((inner, s) => () => withServices(s, inner), own)();
  }
  const ran = asExit(yield exitOf(() => (timeout === undefined ? each() : timeoutOption(timeout, each))));
  if (ran.cause !== undefined) {
    outcome = ran;
    return raise(ran.cause);
  }
  const done: unknown = ran.value;
  const result = timeout === undefined ? done : isSome(done) ? done.value : new TimedOut(timeout);
  if (result instanceof TimedOut) outcome = failed(interrupt(undefined));
  else if (result instanceof Setup) outcome = failed(typeof result.cause === "string" ? fail(result.cause) : result.cause);
  return result;
}

type Problem = Readonly<{ kind: NonNullable<TestResult["kind"]>; message: string }>;

/** How a test body ended: `undefined` when it passed. */
const outcome = (exit: Exit): Problem | undefined => {
  const { cause } = exit;
  if (cause === undefined) {
    const { value } = exit;
    if (value instanceof Setup) {
      const why = value.cause;
      return { kind: "setup", message: typeof why === "string" ? why : (outcome(failed(why))?.message ?? "") };
    }
    if (value instanceof TimedOut) return { kind: "timeout", message: `timed out after ${value.millis} ms` };
    return typeof value === "string" ? { kind: "assertion", message: value } : undefined;
  }
  if (hasDies(cause)) return { kind: "defect", message: describeCause(cause) };
  if (hasInterruptsOnly(cause)) return { kind: "interrupted", message: "interrupted" };
  const reason = cause.reasons.find(isFail);
  if (reason && Core.isAssertionFailed(reason.error)) return { kind: "assertion", message: reason.error._0 };
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

const SNAPSHOTS: object = /*#__PURE__*/ Object.freeze({ key: "Test.snapshots" });

/**
 * `Test.snapshot`: `None` when the snapshot `name` of the test this code runs
 * for holds `text`, or the run wrote it; `Some None` when there is none, and
 * `Some (Some stored)` when it holds another text.
 */
export const testSnapshot = (name: string, text: string): Option<Option<string>> => {
  const check = lookup(SNAPSHOTS);
  if (typeof check !== "function") throw new Error("a snapshot is checked only in the body of a test that `flick test` runs");
  const stored = (check as SnapshotCase)(name, text);
  return stored.kind === "same" ? none : some(stored.kind === "missing" ? none : some(stored.text));
};

/** Runs one test, then closes the suites it ends, and gives its result to `done`. */
function* runOne({ name, test, suites, timeout, snapshot }: Named, seed: number, runs: Map<Suite, SuiteRun>, ctx: Ctx, done: (r: TestResult) => void): Run {
  const mine = suites.flatMap((s) => runs.get(s) ?? []);
  const body =
    test.kind === "effect"
      ? function* (): Run {
          yield snapshot === undefined ? test.task() : new Under(extend(currentCtx(), SNAPSHOTS, snapshot), test.task);
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
  // the tests of a `Test.serial` run one after another in a job of their own
  const tasks = Core.jobsOf(selected).map((job) =>
    function* (): Run {
      for (const index of job) {
        const t = selected[index];
        if (t === undefined) continue;
        hooks?.started(index, t.name);
        yield runOne(t, seed, runs, ctx, (r) => {
          results[index] = r;
          hooks?.finished(index, r);
        });
      }
      return undefined;
    },
  );
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
