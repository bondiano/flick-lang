// Tests and generators as data, whatever runtime runs them: a test is built by
// pure code, and an effect test holds the task of its runtime.
import type { HashDict } from "./data.ts";
import { item } from "../internal.ts";
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
export const random = (seed: number): Rng => {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
};

/** What `Test.skip` and `Test.only` ask of a test and of each test in it. */
export type Mode = "skip" | "only";

/** When a hook of a group runs around the group's selected tests. */
export type When = "before_all" | "before_each" | "after_each" | "after_all";

/** The hooks of a group, each kind in the order written. */
export type Hooks<Task> = Readonly<Record<When, ReadonlyArray<Task>>>;

/**
 * `Test.Test`, whose effect tests and hooks hold a `Task` and whose suites a
 * `L`ayer; `timeout`, of `Test.timeout`, is in milliseconds.
 */
export type Test<Task, L> = (
  | Readonly<{ kind: "pure"; name: string; body: (unit: undefined) => Assertion }>
  | Readonly<{ kind: "effect"; name: string; task: Task }>
  | Readonly<{ kind: "property"; name: string; run: (rng: Rng, index: number) => string | undefined }>
  | Readonly<{ kind: "group"; name: string; tests: ReadonlyArray<Test<Task, L>> }>
  | Readonly<{ kind: "hook"; name: string; when: When; task: Task }>
  | Suite<Task, L>
) & Readonly<{ mode?: Mode; timeout?: number }>;

/**
 * A suite in a run: `Test.suite`, tests that share the services of `layer`,
 * built once for them, or a group with hooks, which run around its tests.
 */
export type Suite<Task, L> = Readonly<{
  kind: "suite";
  name: string;
  layer?: L;
  hooks?: Hooks<Task>;
  tests: ReadonlyArray<Test<Task, L>>;
  mode?: Mode;
  timeout?: number;
}>;

export type Leaf<Task, L> = Exclude<Test<Task, L>, { kind: "group" | "suite" | "hook" }>;

export const testPure = <Task, L>(name: string, body: (unit: undefined) => Assertion): Test<Task, L> =>
  Object.freeze({ kind: "pure", name, body });
export const testGroup = <Task, L>(name: string, tests: ReadonlyArray<Test<Task, L>>): Test<Task, L> =>
  Object.freeze({ kind: "group", name, tests });

/** `Test.skip`: the test, and each test in it, reported as skipped and never run. */
export const testSkip = <Task, L>(test: Test<Task, L>): Test<Task, L> => Object.freeze({ ...test, mode: "skip" });

/** `Test.only`: the test, and each test in it, marked for a run of only the marked tests. */
export const testOnly = <Task, L>(test: Test<Task, L>): Test<Task, L> => Object.freeze({ ...test, mode: "only" });

/** `Test.timeout`: the test, and each test in it without a timeout of its own, given `millis`. */
export const testTimeout = <Task, L>(millis: number, test: Test<Task, L>): Test<Task, L> =>
  Object.freeze({ ...test, timeout: millis });

const hook =
  (when: When) =>
  <Task, L>(task: Task): Test<Task, L> =>
    Object.freeze({ kind: "hook", name: when, when, task });

/** `Test.before_all`, `before_each`, `after_each` and `after_all`: hooks of the group they are in. */
export const testBeforeAll = /*#__PURE__*/ hook("before_all");
export const testBeforeEach = /*#__PURE__*/ hook("before_each");
export const testAfterEach = /*#__PURE__*/ hook("after_each");
export const testAfterAll = /*#__PURE__*/ hook("after_all");

/** The hooks among `tests`, none when there are none. */
const hooksOf = <Task, L>(tests: ReadonlyArray<Test<Task, L>>): Hooks<Task> | undefined => {
  const hooks: Record<When, Array<Task>> = { before_all: [], before_each: [], after_each: [], after_all: [] };
  let any = false;
  for (const t of tests) {
    if (t.kind !== "hook") continue;
    hooks[t.when].push(t.task);
    any = true;
  }
  return any ? hooks : undefined;
};

/** A BigInt as `<digits>n` and a function as `<function>`, not its source. */
const replacePreview = (_: string, v: unknown): unknown => {
  if (typeof v === "bigint") return `${v}n`;
  if (typeof v === "function") return "<function>";
  if (v === null || typeof v !== "object" || "_id" in v) return v;
  const data = v as { _tag?: unknown; value?: unknown; success?: unknown; failure?: unknown };
  switch (data._tag) {
    case "None": return { _id: "Option", _tag: "None" };
    case "Some": return { _id: "Option", _tag: "Some", value: data.value };
    case "Success": return { _id: "Result", _tag: "Success", value: data.success };
    case "Failure": return { _id: "Result", _tag: "Failure", failure: data.failure };
    default: return v;
  }
};

/** A property's case: its JSON. */
const preview = (value: unknown): string => {
  try {
    return JSON.stringify(value, replacePreview) ?? String(value);
  } catch {
    return "<value>";
  }
};

/** A property test: each case draws a value and checks it; a failure names the value. */
export const testProperty = <A, Task, L>(name: string, gen: Gen<A>, check: (value: A) => Assertion): Test<Task, L> =>
  Object.freeze({
    kind: "property",
    name,
    run: (rng: Rng, index: number) => {
      const value = gen(rng);
      const a = check(value);
      return a._tag === PASS ? undefined : `${a._0} (case ${index}: ${preview(value)})`;
    },
  });

const CASES = 100;

/** The checks of a pure or property test: the failed assertion's message, if any. A throw is a defect. */
export const checks = <Task, L>(test: Exclude<Leaf<Task, L>, { kind: "effect" }>, seed: number): string | undefined => {
  if (test.kind === "pure") {
    const a = test.body(undefined);
    return a._tag === PASS ? undefined : a._0;
  }
  for (let i = 0; i < CASES; i++) {
    const failed = test.run(random(seed + i * 0x9e3779b9), i);
    if (failed !== undefined) return failed;
  }
  return undefined;
};

export interface Named<Task, L> {
  readonly name: string;
  readonly test: Leaf<Task, L>;
  /** The suites it is in, outermost first: those of `Test.suite` and the groups with hooks. */
  readonly suites: ReadonlyArray<Suite<Task, L>>;
  /** Whether it or a group it is in is skipped, or marked `only`. */
  readonly skip: boolean;
  readonly only: boolean;
  /** The milliseconds of its innermost `Test.timeout`, if any. */
  readonly timeout?: number | undefined;
}

type Marks = Readonly<{ skip: boolean; only: boolean; timeout?: number | undefined }>;

/**
 * Tests with their full names, `Module > group > name`; the hooks of a list
 * make it, the module's or a group's, a suite of the tests under it, whose
 * timeout, its own or the one around it, its hooks have too.
 */
export const flatten = <Task, L>(
  prefix: string,
  tests: ReadonlyArray<Test<Task, L>>,
  out: Array<Named<Task, L>> = [],
  outer: ReadonlyArray<Suite<Task, L>> = [],
  marks: Marks = { skip: false, only: false },
): Array<Named<Task, L>> => {
  const hooks = hooksOf(tests);
  let suites = outer;
  if (hooks) {
    const { timeout } = marks;
    const level: Suite<Task, L> =
      timeout === undefined ? { kind: "suite", name: prefix, hooks, tests } : { kind: "suite", name: prefix, hooks, tests, timeout };
    suites = [...outer, Object.freeze(level)];
  }
  for (const t of tests) {
    if (t.kind === "hook") continue;
    const name = `${prefix} > ${t.name}`;
    const own = {
      skip: marks.skip || t.mode === "skip",
      only: marks.only || t.mode === "only",
      timeout: t.timeout ?? marks.timeout,
    };
    if (t.kind === "group") flatten(name, t.tests, out, suites, own);
    else if (t.kind === "suite") flatten(name, t.tests, out, [...suites, t], own);
    else out.push({ name, test: t, suites, ...own });
  }
  return out;
};

/** Whether a typed failure is `Test.AssertionFailed`. */
export const isAssertionFailed = (error: unknown): error is Readonly<{ _tag: string; _0: string }> =>
  typeof error === "object" && error !== null && Reflect.get(error, "_tag") === ASSERTION;
