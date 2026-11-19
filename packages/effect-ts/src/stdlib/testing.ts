// Tests on Effect. A test is data built by pure code; `runTests` runs them,
// each in an owner of its own, and reports assertions, typed failures,
// defects and failed cleanup apart.
import { Cause, type Context, type Duration, Effect, Exit, Fiber, Layer, Option, Scope } from "effect";
import * as TestClock from "effect/testing/TestClock";

import { describeCause } from "../cause.ts";
import { flickAddFinalizer } from "../effect/Scope.ts";
import { type Engine, type Suites as SuitesOf, type TestOptions, type TestResult, parseTestArgs, runTestsWith } from "@flick-lang/runtime/core/suites";
import * as Core from "@flick-lang/runtime/core/testing";
import { service } from "../effects.ts";
import { buildInto } from "../layer.ts";
import { Owner, close, inside, ownedExits } from "../owner.ts";
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

/** A `before_*` hook that failed with `cause`: the setup of a test. */
class Setup {
  readonly cause: Cause.Cause<unknown>;
  constructor(cause: Cause.Cause<unknown>) {
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
}

/** The hooks `tasks` run in order: a `Setup` of the first that fails. */
const runHooks = (tasks: ReadonlyArray<Task>): Effect.Effect<Setup | undefined, unknown> =>
  Effect.gen(function* () {
    for (const task of tasks) {
      const exit = yield* Effect.exit(task);
      // an interrupted hook interrupts the test, a failed one fails its setup
      if (Exit.isFailure(exit)) return Cause.hasInterruptsOnly(exit.cause) ? yield* Effect.failCause(exit.cause) : new Setup(exit.cause);
    }
    return undefined;
  });

/** A hook as a finalizer of the scope around: what it fails with is the scope's cleanup failure. */
// oxlint-disable-next-line typescript/no-unsafe-type-assertion -- a failed finalizer fails the closing of its scope
const finalizer = (task: Task): Effect.Effect<void, never, Scope.Scope> => flickAddFinalizer(task as Effect.Effect<void>);

/**
 * The setup of a suite, in its owner: its `after_all` hooks are finalizers,
 * whatever its `before_all` hooks do, then those run, then its layer is built.
 */
const setUp = ({ hooks: own, layer }: Suite, owner: Owner): Effect.Effect<Built, unknown> =>
  inside(
    owner,
    Effect.gen(function* () {
      for (const task of own?.after_all ?? []) yield* finalizer(task);
      const setup = yield* runHooks(own?.before_all ?? []);
      if (setup !== undefined || layer === undefined) return setup;
      return yield* buildInto(layer, owner.resources);
    }),
  );

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
      runs.set(suite, { remaining: 1, suite, owner, built: Effect.runSync(Effect.cached(Effect.exit(setUp(suite, owner)))) });
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
): Effect.Effect<A | Setup | TimedOut, unknown, Scope.Scope> =>
  Effect.gen(function* () {
    const services: Array<Context.Context<unknown>> = [];
    for (const run of runs) {
      const built = yield* run.built;
      if (Exit.isFailure(built)) return yield* Effect.failCause(built.cause);
      if (built.value instanceof Setup) return built.value;
      if (built.value !== undefined) services.push(built.value);
    }
    const each = Effect.gen(function* () {
      for (const { suite } of runs) {
        for (const task of suite.hooks?.after_each ?? []) yield* finalizer(task);
        const setup = yield* runHooks(suite.hooks?.before_each ?? []);
        if (setup !== undefined) return setup;
      }
      return yield* services.reduceRight<Effect.Effect<A, unknown>>((inner, s) => Effect.provideContext(inner, s), body);
    });
    if (timeout === undefined) return yield* each;
    const ended = yield* Effect.timeoutOption(each, timeout);
    return Option.isSome(ended) ? ended.value : new TimedOut(timeout);
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
    if (value instanceof Setup) return { kind: "setup", message: outcome(Exit.failCause(value.cause))?.message ?? "" };
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
    ownedExits(() => runCase(test.kind === "effect" ? Effect.as(test.task, undefined) : checks(test, seed), suites, timeout)),
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
