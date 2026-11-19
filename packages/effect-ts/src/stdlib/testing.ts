// Tests on Effect. A test is data built by pure code; `runTests` runs them,
// each in an owner of its own, and reports assertions, typed failures,
// defects and failed cleanup apart.
import { Cause, Context, type Duration, Effect, Exit, Fiber, Layer, Option, Scope } from "effect";
import * as TestClock from "effect/testing/TestClock";

import { describeCause } from "../cause.ts";
import { LazyCell } from "../effect/Lazy.ts";
import { flickAddFinalizer } from "../effect/Scope.ts";
import { withOwnerEvidence } from "../owner_evidence.ts";
import { type Engine, type Suites as SuitesOf, type TestOptions, type TestResult, parseTestArgs, runTestsWith } from "@flick-lang/runtime/core/suites";
import * as Core from "@flick-lang/runtime/core/testing";
import { service } from "../effects.ts";
import { buildInto } from "../layer.ts";
import { Owner, close, closeOwn, inside, ownedExits } from "../owner.ts";
import { provideNodeServices } from "../runner/host.ts";

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
  testFixture,
  testGroup,
  testOnly,
  testProperty,
  testPure,
  testShare,
  testShared,
  testSkip,
} from "@flick-lang/runtime/core/testing";
export type { Assertion, Gen, Rng } from "@flick-lang/runtime/core/testing";
export { parseTestArgs } from "@flick-lang/runtime/core/suites";
export type { FailureKind, TestOptions, TestResult } from "@flick-lang/runtime/core/suites";

type Task = Effect.Effect<unknown, unknown>;
type AnyLayer = Layer.Layer<unknown, unknown>;

/** `Test.Test`. */
export type Test = Core.Test<Task, AnyLayer>;
type Suite = Core.Suite<Task, AnyLayer>;
type Leaf = Core.Leaf<Task, AnyLayer>;
type Named = Core.Named<Task, AnyLayer>;

export const testEffect = (name: string, task: Task): Test => Object.freeze({ kind: "effect", name, task });

/** `Test.serial`: the test, and each test in it, run one at a time. */
export const testSerial = (test: Test): Test => Core.testSerial(test);

/** `Test.before_all` and the other hooks of the group they are in. */
export const testBeforeAll = (task: Task): Test => Core.testBeforeAll(task);
export const testBeforeEach = (task: Task): Test => Core.testBeforeEach(task);
export const testAfterEach = (task: Task): Test => Core.testAfterEach(task);
export const testAfterAll = (task: Task): Test => Core.testAfterAll(task);

/** `Test.timeout`: the test, and each test in it without a timeout of its own, given `millis`. */
export const testTimeout = (millis: number, test: Test): Test => Core.testTimeout(millis, test);

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

/** A `before_*` hook that failed with `cause`, or ran out of time as `cause` says: the setup of a test. */
class Setup {
  readonly cause: Cause.Cause<unknown> | string;
  constructor(cause: Cause.Cause<unknown> | string) {
    this.cause = cause;
  }
}

const timedOut = (what: string, millis: number): string => `${what} timed out after ${millis} ms`;

/**
 * `task` within `millis`: `None` when they run out first, which interrupts
 * it, also where interruption is masked, as in a finalizer.
 */
const within = <A>(task: Effect.Effect<A, unknown>, millis: number): Effect.Effect<Option.Option<A>, unknown> =>
  Effect.timeoutOption(Effect.interruptible(task), millis);

/** A test whose `before_each` hooks and body ran out of their `millis`. */
class TimedOut {
  readonly millis: number;
  constructor(millis: number) {
    this.millis = millis;
  }
}

/** How a suite's setup ended: the services of its layer, if it has one, or a failed hook. */
type Built = Context.Context<unknown> | Setup | undefined;

/**
 * A suite in a run: an owner of its own, set up before its first selected
 * test, and which closes after its last one.
 */
interface SuiteRun {
  remaining: number;
  readonly suite: Suite;
  readonly owner: Owner;
  /** The setup, run once, its outcome shared by the suite's tests. */
  readonly built: Effect.Effect<Exit.Exit<Built, unknown>>;
  /** Its shared fixtures, each made the first time code uses it. */
  readonly shared: Cells;
}

type Fixture = Core.Fixture<Task>;
type Cell = LazyCell<unknown, unknown, unknown>;

/**
 * The fixtures of a test or a suite's run, and the owner they are made in:
 * a scope of their own, which the scope of the test or the suite closes
 * after the hooks that run when it closes, as those may use them.
 */
type Cells = Readonly<{ cells: Map<Fixture, Cell>; owner: Owner }>;

const cellsIn = (outer: Owner): Cells => ({ cells: new Map(), owner: new Owner(Effect.runSync(Scope.make()), outer.defects) });

/** The finalizer that closes `cells` with `outcome`: it fails as their finalizers do. */
const closing = ({ owner }: Cells, outcome: () => Exit.Exit<unknown, unknown> = () => Exit.void): Effect.Effect<void, never, Scope.Scope> =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a failed finalizer fails the closing of its scope
  flickAddFinalizer(Effect.flatMap(Effect.suspend(() => closeOwn(owner, outcome())), (ended) => ended) as Effect.Effect<void>);

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

const FixturesRef = /*#__PURE__*/ Context.Reference<Fixtures | undefined>("@flick-lang/runtime/Test.fixtures", {
  defaultValue: () => undefined,
});

/** `task`, with `fixtures` for the fixtures it uses. */
const withFixtures = (fixtures: Fixtures, task: Task): Task => Effect.provideService(task, FixturesRef, fixtures);

/** The cell of `fixture`, made the first time: its initializer runs in the scope of the cells. */
const cellOf = ({ cells, owner }: Cells, fixture: Fixture): Cell => {
  const found = cells.get(fixture);
  if (found !== undefined) return found;
  const cell: Cell = new LazyCell(inside(owner, withOwnerEvidence(owner, () => fixture.init)));
  cells.set(fixture, cell);
  return cell;
};

/**
 * `Test.use`: the `Lazy` of `fixture` for the test this code runs for, or
 * for the run of the innermost suite around it that shares it.
 */
export const testUse = (fixture: Fixture): Effect.Effect<Cell> =>
  Effect.flatMap(Effect.service(FixturesRef), (fixtures) => {
    if (fixtures === undefined) return Effect.die(new Error(`the fixture ${fixture.key} runs only in the code of a test or a suite that \`flick test\` runs`));
    if (fixture.shared) {
      const run = fixtures.suites.findLast((r) => r.suite.shares?.has(fixture) === true);
      if (run === undefined) return Effect.die(new Error(`the shared fixture ${fixture.key}, used by code that no suite sharing it runs`));
      return Effect.sync(() => cellOf(run.shared, fixture));
    }
    const { test } = fixtures;
    if (test === undefined) return Effect.die(new Error(`the fixture ${fixture.key} of a test, used by code that runs for no test`));
    return Effect.sync(() => cellOf(test, fixture));
  });

/** The hooks `tasks` run in order, each within `millis` if any: a `Setup` of the first that fails. */
const runHooks = (tasks: ReadonlyArray<Task>, what: string, millis?: number): Effect.Effect<Setup | undefined, unknown> =>
  Effect.gen(function* () {
    for (const task of tasks) {
      const exit = yield* Effect.exit(millis === undefined ? Effect.asSome(task) : within(task, millis));
      // an interrupted hook interrupts the test, a failed one fails its setup
      if (Exit.isFailure(exit)) return Cause.hasInterruptsOnly(exit.cause) ? yield* Effect.failCause(exit.cause) : new Setup(exit.cause);
      if (millis !== undefined && Option.isNone(exit.value)) return new Setup(timedOut(what, millis));
    }
    return undefined;
  });

/**
 * A hook as a finalizer of the scope around, within `millis` if any: what it
 * fails with, or that it ran out of time, is the scope's cleanup failure.
 */
const finalizer = (task: Task, what: string, millis: number | undefined): Effect.Effect<void, never, Scope.Scope> => {
  const bounded =
    millis === undefined
      ? task
      : Effect.flatMap(within(task, millis), (done) => (Option.isSome(done) ? Effect.void : Effect.fail(timedOut(what, millis))));
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a failed finalizer fails the closing of its scope
  return flickAddFinalizer(bounded as Effect.Effect<void>);
};

/**
 * The setup of a suite, in its owner: its `after_all` hooks are finalizers,
 * whatever its `before_all` hooks do, then those run, then its layer is
 * built; its hooks have its timeout, if it has one.
 */
const setUp = ({ hooks: own, layer, timeout }: Suite, owner: Owner, shared: Cells, suites: ReadonlyArray<SuiteRun>): Effect.Effect<Built, unknown> =>
  // the hooks of a suite run in its scope, which their own scopes are in
  inside(
    owner,
    withOwnerEvidence(owner, () =>
      Effect.gen(function* () {
        // its shared fixtures close after its `after_all` hooks, which may use them
        yield* closing(shared);
        // the hooks of a suite see its shared fixtures and those of the suites around it
        const fixtures = new Fixtures(undefined, suites);
        for (const task of own?.after_all ?? []) yield* finalizer(withFixtures(fixtures, task), "after_all", timeout);
        const setup = yield* runHooks((own?.before_all ?? []).map((task) => withFixtures(fixtures, task)), "before_all", timeout);
        if (setup !== undefined || layer === undefined) return setup;
        return yield* buildInto(layer, owner.resources);
      }),
    ),
  );

const suiteRuns = (selected: ReadonlyArray<Named>): Map<Suite, SuiteRun> => {
  const runs = new Map<Suite, SuiteRun>();
  for (const { suites } of selected) {
    for (const [level, suite] of suites.entries()) {
      const run = runs.get(suite);
      if (run !== undefined) {
        run.remaining++;
        continue;
      }
      const owner = new Owner(Effect.runSync(Scope.make()), []);
      const shared = cellsIn(owner);
      // the suites around it, itself last, have their runs by the time it is set up
      const around = Effect.suspend(() => setUp(suite, owner, shared, suites.slice(0, level + 1).flatMap((s) => runs.get(s) ?? [])));
      runs.set(suite, { remaining: 1, suite, owner, built: Effect.runSync(Effect.cached(Effect.exit(around))), shared });
    }
  }
  return runs;
};

/**
 * A test in its scope: the suites it is in set up, outermost first, then,
 * within its `timeout`, the `before_each` hooks of each, outermost first,
 * and `body` with their services; the `after_each` hooks are finalizers of
 * the test's scope.
 */
const runCase = <A>(
  body: Effect.Effect<A, unknown>,
  runs: ReadonlyArray<SuiteRun>,
  timeout: number | undefined,
  owner: Owner,
): Effect.Effect<A | Setup | TimedOut, unknown, Scope.Scope> =>
  Effect.gen(function* () {
    const services: Array<Context.Context<unknown>> = [];
    for (const run of runs) {
      const built = yield* run.built;
      if (Exit.isFailure(built)) return yield* Effect.failCause(built.cause);
      if (built.value instanceof Setup) return built.value;
      if (built.value !== undefined) services.push(built.value);
    }
    // the fixtures of the test, made the first time its code uses each
    const test = cellsIn(owner);
    // how the case ended, which their scope closes with: what it owns sees whether the case passed
    let outcome: Exit.Exit<unknown, unknown> = Exit.void;
    yield* closing(test, () => outcome);
    const each = Effect.gen(function* () {
      for (const [level, { suite }] of runs.entries()) {
        const fixtures = new Fixtures(test, runs.slice(0, level + 1));
        for (const task of suite.hooks?.after_each ?? []) yield* finalizer(withFixtures(fixtures, task), "after_each", suite.timeout);
        // the test's timeout bounds its `before_each` hooks and its body together
        const setup = yield* runHooks((suite.hooks?.before_each ?? []).map((task) => withFixtures(fixtures, task)), "before_each");
        if (setup !== undefined) return setup;
      }
      const own = Effect.provideService(body, FixturesRef, new Fixtures(test, runs));
      return yield* services.reduceRight<Effect.Effect<A, unknown>>((inner, s) => Effect.provideContext(inner, s), own);
    });
    const ran = yield* Effect.exit(timeout === undefined ? Effect.map(each, Option.some) : Effect.timeoutOption(each, timeout));
    if (Exit.isFailure(ran)) {
      outcome = ran;
      return yield* Effect.failCause(ran.cause);
    }
    const result = Option.isSome(ran.value) ? ran.value.value : new TimedOut(timeout ?? 0);
    if (result instanceof TimedOut) outcome = Exit.interrupt(undefined);
    else if (result instanceof Setup) outcome = Exit.failCause(typeof result.cause === "string" ? Cause.fail(result.cause) : result.cause);
    return result;
  });

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
const outcome = (exit: Exit.Exit<string | Setup | TimedOut | undefined, unknown>): Problem | undefined => {
  if (Exit.isSuccess(exit)) {
    const { value } = exit;
    if (value instanceof Setup) {
      const why = value.cause;
      return { kind: "setup", message: typeof why === "string" ? why : (outcome(Exit.failCause(why))?.message ?? "") };
    }
    if (value instanceof TimedOut) return { kind: "timeout", message: `timed out after ${value.millis} ms` };
    return value === undefined ? undefined : { kind: "assertion", message: value };
  }
  const { cause } = exit;
  if (Cause.hasDies(cause)) return { kind: "defect", message: describeCause(cause) };
  if (Cause.hasInterruptsOnly(cause)) return { kind: "interrupted", message: "interrupted" };
  const fail = cause.reasons.find(Cause.isFailReason);
  if (fail && Core.isAssertionFailed(fail.error)) return { kind: "assertion", message: fail.error._0 };
  return { kind: "failure", message: describeCause(cause) };
};

const runOne = ({ name, test, suites, timeout }: Named, seed: number, runs: Map<Suite, SuiteRun>): Effect.Effect<TestResult> => {
  const mine = suites.flatMap((s) => runs.get(s) ?? []);
  const result = runLeaf(name, test, seed, mine, timeout);
  if (mine.length === 0) return result;
  return Effect.flatMap(result, (r) =>
    Effect.map(leave(mine), (failed): TestResult => {
      if (failed === undefined) return r;
      // the suite's release fails the test that ended it
      return { ...r, status: "failed", ...(r.status === "passed" ? { kind: "cleanup", message: failed } : {}), cleanup: failed };
    }),
  );
};

const runLeaf = (
  name: string,
  test: Leaf,
  seed: number,
  suites: ReadonlyArray<SuiteRun>,
  timeout: number | undefined,
): Effect.Effect<TestResult> =>
  Effect.map(
    ownedExits((owner) => runCase(test.kind === "effect" ? Effect.as(test.task, undefined) : checks(test, seed), suites, timeout, owner)),
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
  // the tests and their suites run with the live host services, as a `main` of `Runner.Node`
  const fiber = Effect.runFork(
    provideNodeServices(
      Effect.suspend(() => {
        const results: Array<TestResult> = [];
        const one = (index: number): Effect.Effect<void> => {
          const t = selected[index];
          if (t === undefined) return Effect.void;
          const run = hooks
            ? Effect.suspend(() => {
                hooks.started(index, t.name);
                return Effect.tap(runOne(t, seed, runs), (result) => Effect.sync(() => hooks.finished(index, result)));
              })
            : runOne(t, seed, runs);
          return Effect.map(run, (result) => void (results[index] = result));
        };
        // the tests of a `Test.serial` run one after another in a job of their own
        const all = Effect.forEach(Core.jobsOf(selected), (job) => Effect.forEach(job, one, { discard: true }), {
          concurrency: jobs,
          discard: true,
        });
        return Effect.as(all, results);
      }),
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
