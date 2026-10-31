// Tests and generators. A test is data built by pure code; `runTests` runs
// them, each in an owner of its own, and reports assertions, typed
// failures, defects and failed cleanup apart.
import { Cause, type Duration, Effect, Exit, Fiber, Layer } from "effect";
import * as TestClock from "effect/testing/TestClock";

import { describeCause } from "../cause.ts";
import { service } from "../effects.ts";
import type { HashDict, Tagged } from "../data.ts";
import { item } from "../internal.ts";
import { ownedExits } from "../owner.ts";
import { tag } from "../tags.ts";

const ASSERTION = /*#__PURE__*/ tag("Test.AssertionFailed.AssertionFailed");
const PASS = /*#__PURE__*/ tag("Test.Assertion.Pass");

/** `Test.Assertion`: a pass, or a failure with its message. */
export type Assertion = Readonly<{ _tag: string; _0?: string }>;

/** A number in `[0, 1)`. */
export type Rng = () => number;

/** `Gen a`: draws a value from `rng`. */
export type Gen<A> = (rng: Rng) => A;

export const genConstant =
  <A>(value: A): Gen<A> =>
  () =>
    value;
export const genNumber =
  (low: number, high: number): Gen<number> =>
  (rng) =>
    low + rng() * (high - low);
export const genInt =
  (low: number, high: number): Gen<number> =>
  (rng) =>
    low + Math.floor(rng() * (high - low + 1));
export const genBool: Gen<boolean> = (rng) => rng() < 0.5;
export const genString =
  (max: number): Gen<string> =>
  (rng) => {
    const length = Math.floor(rng() * (max + 1));
    return Array.from({ length }, () => String.fromCharCode(32 + Math.floor(rng() * 95))).join("");
  };
export const genList =
  <A>(max: number, gen: Gen<A>): Gen<Array<A>> =>
  (rng) => {
    const length = Math.floor(rng() * (max + 1));
    return Array.from({ length }, () => gen(rng));
  };
export const genOneOf = <A>(values: ReadonlyArray<A>): Gen<A> => {
  if (values.length === 0) throw new RangeError("Gen.one_of needs at least one value");
  return (rng) => item(values, Math.floor(rng() * values.length));
};
export const genPair =
  <A, B>(a: Gen<A>, b: Gen<B>): Gen<[A, B]> =>
  (rng) => [a(rng), b(rng)];
export const genMap =
  <A, B>(f: (a: A) => B, gen: Gen<A>): Gen<B> =>
  (rng) =>
    f(gen(rng));

/**
 * `Gen.function`: a pure function whose result for an argument is drawn
 * from `gen` with a seed of the argument's hash, the same each call.
 */
export const genFunction =
  <A, B>(dHash: HashDict<A>, gen: Gen<B>): Gen<(a: A) => B> =>
  (rng) => {
    const seed = Math.floor(rng() * 2 ** 32);
    return (a) => gen(random(seed ^ dHash.hash(a)));
  };

/** mulberry32: a small seeded generator, enough for test data. */
const random = (seed: number): Rng => {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};

/** `Test.Test`. */
export type Test =
  | Readonly<{ kind: "pure"; name: string; body: (unit: undefined) => Assertion }>
  | Readonly<{ kind: "effect"; name: string; task: Effect.Effect<unknown, unknown> }>
  | Readonly<{ kind: "property"; name: string; run: (rng: Rng, index: number) => string | undefined }>
  | Readonly<{ kind: "group"; name: string; tests: ReadonlyArray<Test> }>;

type Leaf = Exclude<Test, { kind: "group" }>;

export const testPure = (name: string, body: (unit: undefined) => Assertion): Test =>
  Object.freeze({ kind: "pure", name, body });
export const testEffect = (name: string, task: Effect.Effect<unknown, unknown>): Test =>
  Object.freeze({ kind: "effect", name, task });
/** The service `Test.VirtualTime`: a test that may move its clock. */
const VirtualTime = /*#__PURE__*/ service("Test.VirtualTime");

/** `Test.clock_test`: an effect test on a TestClock of its own, made for each run. */
export const testClock = (name: string, task: Effect.Effect<unknown, unknown, unknown>): Test =>
  testEffect(
    name,
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the layer provides every service the row names
    Effect.provide(task, Layer.merge(TestClock.layer(), Layer.succeed(VirtualTime, {}))) as Effect.Effect<unknown, unknown>,
  );

/** `Test.adjust`: moves the test's clock forward. */
export const testAdjust = (duration: Duration.Duration): Effect.Effect<void> => TestClock.adjust(duration);

/** `Test.set_time`: sets the test's clock to `millis` since the epoch. */
export const testSetTime = (millis: number): Effect.Effect<void> => TestClock.setTime(millis);

/** A property test: each case draws a value and checks it; a failure names the value. */
export const testProperty = <A>(name: string, gen: Gen<A>, check: (value: A) => Assertion): Test =>
  Object.freeze({
    kind: "property",
    name,
    run: (rng: Rng, index: number) => {
      const value = gen(rng);
      const a = check(value);
      return a._tag === PASS ? undefined : `${a._0} (case ${index}: ${preview(value)})`;
    },
  });
export const testGroup = (name: string, tests: ReadonlyArray<Test>): Test => Object.freeze({ kind: "group", name, tests });

const CASES = 100;

interface Named {
  readonly name: string;
  readonly test: Leaf;
}

/** Tests with their full names, `Module > group > name`. */
const flatten = (prefix: string, tests: ReadonlyArray<Test>, out: Array<Named> = []): Array<Named> => {
  for (const t of tests) {
    const name = `${prefix} > ${t.name}`;
    if (t.kind === "group") flatten(name, t.tests, out);
    else out.push({ name, test: t });
  }
  return out;
};

/** A BigInt as `<digits>n` and a function as `<function>`, not its source. */
const replacePreview = (_: string, v: unknown): unknown =>
  typeof v === "bigint" ? `${v}n` : typeof v === "function" ? "<function>" : v;

/** A property's case: its JSON. */
const preview = (value: unknown): string => {
  try {
    return JSON.stringify(value, replacePreview) ?? String(value);
  } catch {
    return "<value>";
  }
};

/** The checks of a pure or property test, as an effect whose throw is a defect. */
const checks = (test: Exclude<Leaf, { kind: "effect" }>, seed: number): Effect.Effect<string | undefined> =>
  Effect.sync(() => {
    if (test.kind === "pure") {
      const a = test.body(undefined);
      return a._tag === PASS ? undefined : a._0;
    }
    for (let i = 0; i < CASES; i++) {
      const failed = test.run(random(seed + i * 0x9e3779b9), i);
      if (failed !== undefined) return failed;
    }
    return undefined;
  });

type FailureKind = "assertion" | "defect" | "interrupted" | "failure" | "cleanup";

interface Problem {
  readonly kind: FailureKind;
  readonly message: string;
}

const isAssertionFailed = (error: unknown): error is Tagged<typeof ASSERTION, string> =>
  typeof error === "object" && error !== null && Reflect.get(error, "_tag") === ASSERTION;

/** How a test body ended: `undefined` when it passed. */
const outcome = (exit: Exit.Exit<string | undefined, unknown>): Problem | undefined => {
  if (Exit.isSuccess(exit)) {
    return exit.value === undefined ? undefined : { kind: "assertion", message: exit.value };
  }
  const { cause } = exit;
  if (Cause.hasDies(cause)) return { kind: "defect", message: describeCause(cause) };
  if (Cause.hasInterruptsOnly(cause)) return { kind: "interrupted", message: "interrupted" };
  const fail = cause.reasons.find(Cause.isFailReason);
  if (fail && isAssertionFailed(fail.error)) return { kind: "assertion", message: fail.error._0 };
  return { kind: "failure", message: describeCause(cause) };
};

/** The result of one test, as the reporters write it. */
export interface TestResult {
  readonly name: string;
  readonly status: "passed" | "failed";
  readonly kind?: FailureKind;
  readonly message?: string;
  readonly cleanup?: string;
}

const runOne = ({ name, test }: Named, seed: number): Effect.Effect<TestResult> =>
  Effect.map(
    ownedExits(() => (test.kind === "effect" ? Effect.as(test.task, undefined) : checks(test, seed))),
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

/** Options of `flick test`. */
export interface TestOptions {
  readonly filter?: string;
  readonly jobs?: number;
  readonly seed?: number | undefined;
  readonly reporter?: string;
  readonly watched?: boolean;
  readonly color?: boolean;
  readonly write?: (text: string) => void;
}

/** Options of `flick test`, from the runner's arguments. */
export const parseTestArgs = (argv: ReadonlyArray<string>): TestOptions => {
  const options: {
    filter: string;
    jobs: number;
    seed: number | undefined;
    reporter: string;
    watched: boolean;
    color: boolean;
  } = {
    filter: "",
    jobs: 1,
    seed: undefined,
    reporter: "human",
    watched: false,
    color: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const value = argv[i + 1] ?? "";
    if (flag === "--filter") options.filter = value;
    else if (flag === "--jobs") options.jobs = Number(value);
    else if (flag === "--seed") options.seed = Number(value);
    else if (flag === "--reporter") options.reporter = value;
    else if (flag === "--color") options.color = value === "always";
    else if (flag === "--watched") {
      options.watched = true;
      continue;
    } else continue;
    i++;
  }
  return options;
};

const humanResult = (
  result: TestResult,
  completed: number,
  total: number,
  elapsedMs: number,
  color: boolean,
): string => {
  const reset = color ? "\x1b[0m" : "";
  const dim = color ? "\x1b[2m" : "";
  const marker =
    result.status === "passed" ? `${color ? "\x1b[32m" : ""}✓${reset}` : `${color ? "\x1b[31m" : ""}✗${reset}`;
  const duration = Math.max(0, Math.floor(elapsedMs));
  let text = `  ${marker} ${dim}[${completed}/${total}]${reset} ${result.name}  ${dim}(${duration} ms)${reset}\n`;
  if (result.status === "failed") {
    text += `    ${`${result.kind ?? "failure"}: ${result.message ?? ""}`.replaceAll("\n", "\n    ")}\n`;
    if (result.cleanup && result.kind !== "cleanup") {
      text += `    ${`and cleanup failed: ${result.cleanup}`.replaceAll("\n", "\n    ")}\n`;
    }
  }
  return text;
};

const report = (
  results: ReadonlyArray<TestResult>,
  seed: number,
  reporter: string | undefined,
  write: (text: string) => void,
  color: boolean,
  elapsedMs: number,
): number => {
  const failed = results.filter((r) => r.status === "failed").length;
  const passed = results.length - failed;
  if (reporter === "json") {
    write(`${JSON.stringify({ seed, passed, failed, tests: results })}\n`);
    return failed;
  }
  const reset = color ? "\x1b[0m" : "";
  const dim = color ? "\x1b[2m" : "";
  const passedColor = color && passed > 0 ? "\x1b[32m" : dim;
  const failedColor = color && failed > 0 ? "\x1b[31m" : dim;
  write(`\n  Tests  ${passedColor}${passed}${reset} passed · ${failedColor}${failed}${reset} failed · ${results.length} total\n`);
  write(`  Time   ${dim}${Math.max(0, Math.floor(elapsedMs))} ms${reset}\n`);
  if (failed > 0) write(`seed: ${seed} (rerun with --seed ${seed})\n`);
  return failed;
};

/** The `tests` of each test module, by module name. */
export type Suites = ReadonlyArray<readonly [module: string, tests: ReadonlyArray<Test> | undefined]>;

const isEpipe = (error: unknown): boolean =>
  typeof error === "object" && error !== null && Reflect.get(error, "code") === "EPIPE";

/**
 * Runs the `tests` of each module, at most `jobs` at a time, and sets the
 * exit code: 0 when all passed, 1 otherwise, 130 when SIGINT (or, for a
 * `watched` run, the end of stdin) stopped the run after every running
 * test's cleanup.
 */
export const runTests = async (
  modules: Suites,
  options: TestOptions = parseTestArgs(process.argv.slice(2)),
): Promise<void> => {
  const write = options.write ?? ((text: string): void => void process.stdout.write(text));
  // a watcher that went away closes the pipe: nothing is left to report to
  process.stdout.on("error", (error) => {
    if (!isEpipe(error)) throw error;
  });
  const seed =
    options.seed !== undefined && Number.isInteger(options.seed) ? options.seed : Math.floor(Math.random() * 2 ** 31);
  const jobs = options.jobs !== undefined && Number.isInteger(options.jobs) && options.jobs > 0 ? options.jobs : 1;
  const selected = modules
    .flatMap(([module, tests]) => flatten(module, tests ?? []))
    .filter(({ name }) => name.includes(options.filter ?? ""));
  const human = options.reporter !== "json";
  const color = options.color ?? false;
  let completed = 0;
  if (human) {
    const title = color ? "\x1b[1;36mflick test\x1b[0m" : "flick test";
    write(`  ${title}  ${selected.length} tests\n\n`);
  }
  const started = human ? performance.now() : 0;
  const fiber = Effect.runFork(
    Effect.forEach(
      selected,
      (t) =>
        human
          ? Effect.suspend(() => {
              const testStarted = performance.now();
              return Effect.map(runOne(t, seed), (result) => {
                write(humanResult(result, ++completed, selected.length, performance.now() - testStarted, color));
                return result;
              });
            })
          : runOne(t, seed),
      { concurrency: jobs },
    ),
  );
  let interrupted = false;
  const stop = (): void => {
    interrupted = true;
    Effect.runFork(Fiber.interrupt(fiber));
  };
  process.once("SIGINT", stop);
  // a watcher stops the run by closing stdin, which works on every platform
  if (options.watched) process.stdin.once("end", stop).resume();
  const exit = await new Promise<Exit.Exit<Array<TestResult>>>((resolve) => fiber.addObserver(resolve));
  process.off("SIGINT", stop);
  if (options.watched) process.stdin.off("end", stop).destroy();
  if (Exit.isFailure(exit)) {
    write(interrupted ? "interrupted: every running test closed its scope\n" : `${Cause.pretty(exit.cause)}\n`);
    process.exitCode = interrupted ? 130 : 1;
    return;
  }
  const failed = report(exit.value, seed, options.reporter, write, color, human ? performance.now() - started : 0);
  process.exitCode = failed > 0 ? 1 : 0;
};
