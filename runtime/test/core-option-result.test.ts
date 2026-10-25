// The Effect-free `core/` bindings of Option, Result, Bool and Number agree
// with Effect's, called the way compiled Flick calls them: the declared arity
// first, then the remaining arguments one at a time.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import { Number as Num } from "effect";

import * as CoreBoolean from "../src/core/Boolean.ts";
import { err, none, ok, some } from "../src/core/data.ts";
import * as CoreNumber from "../src/core/Number.ts";
import * as CoreOption from "../src/core/Option.ts";
import * as CoreResult from "../src/core/Result.ts";
import * as EffBoolean from "../src/effect/Boolean.ts";
import * as EffNumber from "../src/effect/Number.ts";
import * as EffOption from "../src/effect/Option.ts";
import { toOrdering } from "../src/effect/ordering.ts";
import * as EffResult from "../src/effect/Result.ts";

type Module = Readonly<Record<string, unknown>>;
type Fn = (...args: Array<unknown>) => unknown;

/** Effect's Option and Result keep `_tag` on their prototype: as plain data, recursively. */
const plain = (v: unknown): unknown => {
  if (Array.isArray(v)) return v.map(plain);
  if (typeof v !== "object" || v === null || !("_tag" in v)) return v;
  switch (v._tag) {
    case "None":
      return { _tag: "None" };
    case "Some":
      return "value" in v ? { _tag: "Some", value: plain(v.value) } : v;
    case "Success":
      return "success" in v ? { _tag: "Success", success: plain(v.success) } : v;
    case "Failure":
      return "failure" in v ? { _tag: "Failure", failure: plain(v.failure) } : v;
    default:
      return v;
  }
};

/** `f(...stages[0])(...stages[1])…`. */
const call = (f: unknown, stages: ReadonlyArray<ReadonlyArray<unknown>>): unknown =>
  stages.reduce<unknown>((g, args) => {
    assert.equal(typeof g, "function");
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- checked a function just above
    return (g as Fn)(...args);
  }, f);

const get = (m: Module, name: string): unknown => {
  assert.ok(Object.hasOwn(m, name) || name in m, `missing export ${name}`);
  return m[name];
};

type Cases = Readonly<Record<string, ReadonlyArray<ReadonlyArray<ReadonlyArray<unknown>>>>>;

const agree = (core: Module, effect: Module, cases: Cases): void => {
  for (const [name, calls] of Object.entries(cases)) {
    for (const stages of calls) {
      const expected = plain(call(get(effect, name), stages));
      assert.deepStrictEqual(plain(call(get(core, name), stages)), expected, `${name} ${JSON.stringify(stages)}`);
    }
  }
};

/** The JS names `file` binds from `@flick-lang/runtime/effect/<module>`. */
const bound = (file: string, module: string): Array<string> => {
  const text = readFileSync(new URL(`../../flick-core/src/types/builtins/${file}`, import.meta.url), "utf8");
  const re = new RegExp(`foreign \\w+ \\w+ from "@flick-lang/runtime/effect/${module}" "(\\w+)"`, "g");
  return [...text.matchAll(re)].map((m) => m[1] ?? "").toSorted();
};

const coverage: ReadonlyArray<readonly [string, string, Module]> = [
  ["Option.fl", "Option", CoreOption],
  ["Result.fl", "Result", CoreResult],
  ["Bool.fl", "Boolean", CoreBoolean],
  ["Number.fl", "Number", CoreNumber],
];

test("core exports exactly the names the .fl files bind", () => {
  for (const [file, module, core] of coverage) {
    assert.deepStrictEqual(Object.keys(core).toSorted(), bound(file, module), file);
  }
});

const opts = [none, some(0), some(1), some(-0), some(NaN), some(some(2)), some(none)];
const results = [ok(1), err("e"), ok(-0), err(none)];
const isPos = (n: number) => n > 0;
const toResult = (n: number) => (n > 0 ? ok(n * 2) : err(`neg ${n}`));
const toOption = (n: number) => (n > 0 ? some(n + 1) : none);
const add = (a: number, b: number) => a + b;
const sub = (a: number, b: number) => a - b;
const optionArgs = opts.map((o) => [[o]]);

/** A call per value: the stage `first`, then the value. */
const over = (values: ReadonlyArray<unknown>, first: ReadonlyArray<unknown>) => values.map((v) => [first, [v]]);

test("Option agrees with Effect", () => {
  agree(CoreOption, EffOption, {
    all: [[[[]]], [[[some(1), some(2)]]], [[[some(1), none, some(3)]]], [[[none]]]],
    as: over(opts, ["b"]),
    asVoid: optionArgs,
    composeK: [1, 0, -1].map((n) => [[toOption], [toOption], [n]]),
    containsWith: opts.flatMap((o) => [0, 1, NaN].map((x) => [[Object.is], [x], [o]])),
    exists: over(opts, [isPos]),
    filter: over(opts, [isPos]),
    filterMap: over(opts, [toResult]),
    firstSomeOf: [[], [none], [none, none], [none, some(1), some(2)], [some(0)]].map((xs) => [[xs]]),
    flatten: [none, some(none), some(some(1))].map((o) => [[o]]),
    lift2: opts.flatMap((a) => opts.map((b) => [[sub], [a], [b]])),
    liftPredicate: [-1, 0, 2, NaN].map((n) => [[isPos], [n]]),
    match: over(opts, [{ onNone: () => "none", onSome: (x: unknown) => [x] }]),
    orElseResult: [...over(opts, [() => none]), ...over(opts, [() => some("b")])],
    orElseSome: over(opts, [() => 9]),
    partitionMap: over(opts, [toResult]),
    product: opts.flatMap((a) => opts.map((b) => [[a], [b]])),
    productMany: [...over(opts, [[]]), ...over(opts, [[some(2), some(3)]]), ...over(opts, [[some(2), none]])],
    reduceCompact: [[[0, add], [[]]], [[10, sub], [[some(1), none, some(2)]]], [[0, add], [[none, none]]]],
    tap: over(opts, [toOption]),
    zipLeft: opts.flatMap((a) => opts.map((b) => [[a], [b]])),
    zipRight: opts.flatMap((a) => opts.map((b) => [[a], [b]])),
    zipWith: opts.flatMap((a) => opts.map((b) => [[a, sub], [b]])),
  });
  assert.deepStrictEqual(plain(CoreOption.void), plain(EffOption.void));
});

test("Result agrees with Effect", () => {
  agree(CoreResult, EffResult, {
    all: [[[]], [[ok(1), ok(2)]], [[ok(1), err("a"), err("b")]]].map((xs) => [xs]),
    filterOrFail: [...over(results, [isPos, (n: number) => `bad ${n}`]), [[isPos, String], [ok(-2)]]],
    flip: results.map((r) => [[r]]),
    getFailure: results.map((r) => [[r]]),
    liftPredicate: [-1, 0, 3, NaN].map((n) => [[isPos, (x: number) => `bad ${x}`], [n]]),
    mapBoth: over(results, [{ onFailure: (e: unknown) => [e], onSuccess: (a: unknown) => ({ a }) }]),
    match: over(results, [{ onFailure: (e: unknown) => ["f", e], onSuccess: (a: unknown) => ["s", a] }]),
    merge: results.map((r) => [[r]]),
    orElse: [...over(results, [(e: unknown) => ok(["r", e])]), ...over(results, [(e: unknown) => err(["e", e])])],
    succeedSome: [[[1]], [[none]]],
    transposeOption: [none, some(ok(1)), some(err("e")), some(ok(none))].map((o) => [[o]]),
    transposeMapOption: over(opts, [toResult]),
  });
  for (const name of ["failVoid", "void", "succeedNone"]) {
    assert.deepStrictEqual(plain(get(CoreResult, name)), plain(get(EffResult, name)), name);
  }
});

test("Bool agrees with Effect", () => {
  const bools = [false, true];
  const pairs = bools.flatMap((that) => bools.map((self) => [[that], [self]]));
  agree(CoreBoolean, EffBoolean, {
    not: bools.map((b) => [[b]]),
    and: pairs,
    nand: pairs,
    or: pairs,
    nor: pairs,
    xor: pairs,
    eqv: pairs,
    implies: pairs,
    every: [[[]], [[true]], [[true, false]], [[false, false]]].map((xs) => [xs]),
    some: [[[]], [[false]], [[true, false]], [[false, false]]].map((xs) => [xs]),
    match: bools.map((b) => [[{ onFalse: () => "f", onTrue: () => "t" }], [b]]),
  });
  const reducers = [
    [CoreBoolean.ReducerAnd, EffBoolean.ReducerAnd],
    [CoreBoolean.ReducerOr, EffBoolean.ReducerOr],
  ] as const;
  for (const [core, eff] of reducers) {
    assert.equal(core.initialValue, eff.initialValue);
    for (const a of bools) for (const b of bools) assert.equal(core.combine(a, b), eff.combine(a, b));
    for (const xs of [[], [true], [true, false], [false, false]]) assert.equal(core.combineAll(xs), eff.combineAll(xs));
  }
});

test("Number agrees with Effect", () => {
  const nums = [0, -0, 1, -1, 2.5, -2.5, 0.1, 0.3, 1e21, 1e-7, 3, -7, Infinity, -Infinity, NaN, Number.MIN_VALUE];
  const pairs = nums.flatMap((that) => nums.map((self) => [[that], [self]]));
  const ones = nums.map((n) => [[n]]);
  const bounds = [
    { minimum: 0, maximum: 1 },
    { minimum: -1, maximum: -0 },
    { minimum: NaN, maximum: 2 },
    { minimum: 0, maximum: NaN },
    { minimum: 3, maximum: 1 },
  ];
  const ranged = bounds.flatMap((b) => nums.map((n) => [[b], [n]]));
  agree(CoreNumber, EffNumber, {
    min: pairs,
    max: pairs,
    parse: ["", "  ", "1", " 2 ", "1e3", "0x10", "-0", "NaN", "Infinity", "-Infinity", "abc", "1_0", "nan"].map((s) => [[s]]),
    sum: pairs,
    subtract: pairs,
    multiply: pairs,
    divide: pairs,
    remainder: [...pairs, [[0.1], [0.3]], [[0.1], [-0.3]], [[0.2], [1.1]], [[3], [-0.5]], [[1e-7], [1e-300]]],
    increment: ones,
    decrement: ones,
    sumAll: [[], [1, 2, 3], [0.1, 0.2], [NaN, 1], [-0]].map((xs) => [[xs]]),
    multiplyAll: [[], [2, 3], [Infinity, 0], [NaN, 0], [-0], [-1, -0]].map((xs) => [[xs]]),
    isLessThan: pairs,
    isLessThanOrEqualTo: pairs,
    isGreaterThan: pairs,
    isGreaterThanOrEqualTo: pairs,
    between: ranged,
    clamp: ranged,
    nextPow2: [...ones, [[5]], [[1024]], [[1025]]],
    round: [...pairs, [[2], [1.23456]], [[0], [2.5]], [[1], [-2.45]], [[-1], [1234]]],
  });
  for (const n of nums) assert.equal(CoreNumber.flickSign(n), toOrdering(Num.sign(n)), String(n));
});
