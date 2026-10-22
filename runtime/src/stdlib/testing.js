// Tests and generators. A test is data built by pure code; `runTests` runs
// them, each in an owner of its own, and reports assertions, typed
// failures, defects and failed cleanup apart.
import { Cause, Effect, Exit, Fiber } from "effect";

import { describeCause } from "../cause.js";
import { ownedExits } from "../owner.js";

const ASSERTION = "Test.AssertionFailed.AssertionFailed";
const PASS = "Test.Assertion.Pass";

/** A generator draws from `rng`, a function giving numbers in `[0, 1)`. */
export const genConstant = (value) => () => value;
export const genNumber = (low, high) => (rng) => low + rng() * (high - low);
export const genInt = (low, high) => (rng) => low + Math.floor(rng() * (high - low + 1));
export const genBool = (rng) => rng() < 0.5;
export const genString = (max) => (rng) => {
  const length = Math.floor(rng() * (max + 1));
  return Array.from({ length }, () => String.fromCharCode(32 + Math.floor(rng() * 95))).join("");
};
export const genList = (max, gen) => (rng) => {
  const length = Math.floor(rng() * (max + 1));
  return Array.from({ length }, () => gen(rng));
};
export const genOneOf = (values) => {
  if (values.length === 0) throw new RangeError("Gen.one_of needs at least one value");
  return (rng) => values[Math.floor(rng() * values.length)];
};
export const genPair = (a, b) => (rng) => [a(rng), b(rng)];
export const genMap = (f, gen) => (rng) => f(gen(rng));

/** mulberry32: a small seeded generator, enough for test data. */
const random = (seed) => {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};

export const testPure = (name, body) => Object.freeze({ kind: "pure", name, body });
export const testEffect = (name, task) => Object.freeze({ kind: "effect", name, task });
export const testProperty = (name, gen, check) => Object.freeze({ kind: "property", name, gen, check });
export const testGroup = (name, tests) => Object.freeze({ kind: "group", name, tests });

const CASES = 100;

/** Tests with their full names, `Module > group > name`. */
const flatten = (prefix, tests, out = []) => {
  for (const t of tests) {
    const name = `${prefix} > ${t.name}`;
    if (t.kind === "group") flatten(name, t.tests, out);
    else out.push({ name, test: t });
  }
  return out;
};

const preview = (value) => {
  try {
    return JSON.stringify(value, (_, v) => (typeof v === "bigint" ? `${v}n` : v)) ?? String(value);
  } catch {
    return "<value>";
  }
};

/** The checks of a pure or property test, as an effect whose throw is a defect. */
const checks = (test, seed) =>
  Effect.sync(() => {
    if (test.kind === "pure") {
      const a = test.body(undefined);
      return a._tag === PASS ? undefined : a._0;
    }
    for (let i = 0; i < CASES; i++) {
      const value = test.gen(random(seed + i * 0x9e3779b9));
      const a = test.check(value);
      if (a._tag !== PASS) return `${a._0} (case ${i}: ${preview(value)})`;
    }
    return undefined;
  });

/** How a test body ended: `undefined` when it passed. */
const outcome = (exit) => {
  if (Exit.isSuccess(exit)) {
    return exit.value === undefined ? undefined : { kind: "assertion", message: exit.value };
  }
  const { cause } = exit;
  if (Cause.hasDies(cause)) return { kind: "defect", message: describeCause(cause) };
  if (Cause.hasInterruptsOnly(cause)) return { kind: "interrupted", message: "interrupted" };
  const fail = cause.reasons.find(Cause.isFailReason);
  if (fail?.error?._tag === ASSERTION) return { kind: "assertion", message: fail.error._0 };
  return { kind: "failure", message: describeCause(cause) };
};

const runOne = ({ name, test }, seed) =>
  Effect.map(
    ownedExits(() => (test.kind === "effect" ? Effect.as(test.task, undefined) : checks(test, seed))),
    ({ exit, cleanup }) => {
      const body = outcome(exit);
      const result = { name, status: body ? "failed" : "passed" };
      if (body) Object.assign(result, body);
      if (Exit.isFailure(cleanup)) {
        result.status = "failed";
        result.cleanup = describeCause(cleanup.cause);
        if (!body) Object.assign(result, { kind: "cleanup", message: result.cleanup });
      }
      return result;
    },
  );

/** Options of `flick test`, from the runner's arguments. */
export const parseTestArgs = (argv) => {
  const options = { filter: "", jobs: 1, seed: undefined, reporter: "human" };
  for (let i = 0; i < argv.length; i++) {
    const [flag, value] = [argv[i], argv[i + 1]];
    if (flag === "--filter") options.filter = value;
    else if (flag === "--jobs") options.jobs = Number(value);
    else if (flag === "--seed") options.seed = Number(value);
    else if (flag === "--reporter") options.reporter = value;
    else continue;
    i++;
  }
  return options;
};

const report = (results, seed, reporter, write) => {
  const failed = results.filter((r) => r.status === "failed").length;
  const passed = results.length - failed;
  if (reporter === "json") {
    write(`${JSON.stringify({ seed, passed, failed, tests: results })}\n`);
    return failed;
  }
  for (const r of results) {
    if (r.status === "passed") write(`ok    ${r.name}\n`);
    else {
      write(`FAIL  ${r.name}: ${r.kind}: ${r.message}\n`);
      if (r.cleanup && r.kind !== "cleanup") write(`      and cleanup failed: ${r.cleanup}\n`);
    }
  }
  write(`\n${passed} passed, ${failed} failed\n`);
  if (failed > 0) write(`seed: ${seed} (rerun with --seed ${seed})\n`);
  return failed;
};

/**
 * Runs the `tests` of each module, at most `jobs` at a time, and sets the
 * exit code: 0 when all passed, 1 otherwise, 130 when SIGINT stopped the
 * run after every running test's cleanup.
 */
export const runTests = async (modules, options = parseTestArgs(process.argv.slice(2))) => {
  const write = options.write ?? ((text) => process.stdout.write(text));
  const seed = Number.isInteger(options.seed) ? options.seed : Math.floor(Math.random() * 2 ** 31);
  const jobs = Number.isInteger(options.jobs) && options.jobs > 0 ? options.jobs : 1;
  const selected = modules
    .flatMap(([module, tests]) => flatten(module, tests ?? []))
    .filter(({ name }) => name.includes(options.filter ?? ""));
  const fiber = Effect.runFork(
    Effect.forEach(selected, (t) => runOne(t, seed), { concurrency: jobs }),
  );
  let interrupted = false;
  const stop = () => {
    interrupted = true;
    Effect.runFork(Fiber.interrupt(fiber));
  };
  process.once("SIGINT", stop);
  const exit = await new Promise((resolve) => fiber.addObserver(resolve));
  process.off("SIGINT", stop);
  if (Exit.isFailure(exit)) {
    write(interrupted ? "interrupted: every running test closed its scope\n" : `${Cause.pretty(exit.cause)}\n`);
    process.exitCode = interrupted ? 130 : 1;
    return;
  }
  const failed = report(exit.value, seed, options.reporter, write);
  process.exitCode = failed > 0 ? 1 : 0;
};
