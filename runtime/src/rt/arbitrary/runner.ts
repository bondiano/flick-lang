// The combinators and the runner of Arbitrary: attempts at progressive
// sizes, discards, shrinking of the first failure and the replay of it.
import { fromReasons, isFail, raise, unwinds } from "../cause.ts";
import { type Run, asExit, caught } from "../fiber.ts";
import { compileAst } from "./compile.ts";
import { astOf } from "./ast.ts";
import {
  type Gen, type Pull, type Sample, NONE, arraySample, concatPulls, filterMapSample, filterSample,
  generateProduct, lazyShrinks, makeGen, mapSample, productSample, pullEach, sampleOf,
} from "./model.ts";
import { type GenState, type Seed, attemptRandom, hashSeed, randomLength, shuffle, withRandom, withSize } from "./random.ts";
import type { Witness } from "../schema.ts";

/** The values of a schema's type. */
export const schemaGen = (schema: Witness): Gen => compileAst(astOf(schema));

/** The values of a schema, each shrinking to the valid candidates of `shrink`. */
export const schemaGenWith = (schema: Witness, shrink: (value: unknown) => ReadonlyArray<unknown>): Gen => {
  const compiled = schemaGen(schema);
  const validated = (value: unknown): Sample => lazyShrinks(value, () => pullEach(shrink(value), (c) => {
    const valid = schema.value(c, "");
    return valid._tag === "Success" ? validated(valid.success) : null;
  }));
  return makeGen(compiled.minCost, (st) => {
    const s = compiled.generate(st);
    return s === null || !st.shrinks ? s : validated(s.value);
  });
};

export const constantGen = (value: unknown): Gen => makeGen(0, () => sampleOf(value));

export const mapGen = (self: Gen, f: (value: unknown) => unknown): Gen => makeGen(self.minCost, (st) => {
  const s = self.generate(st);
  return s === null ? null : mapSample(s, f);
});

/** A value the predicate rejects is a discard. */
export const filterGen = (self: Gen, keep: (value: unknown) => boolean): Gen => makeGen(self.minCost, (st) => {
  const s = self.generate(st);
  if (s === null) return null;
  if (!st.shrinks) return keep(s.value) ? s : null;
  return filterSample(s, keep);
});

/** `f` gives the value kept, or `NONE` for a discard. */
export const filterMapGen = (self: Gen, f: (value: unknown) => unknown): Gen => makeGen(self.minCost, (st) => {
  const s = self.generate(st);
  if (s === null) return null;
  if (st.shrinks) return filterMapSample(s, f);
  const kept = f(s.value);
  return kept === NONE ? null : sampleOf(kept);
});

export const arrayGen = (item: Gen, minimum: number, maximum: number): Gen => {
  if (!Number.isInteger(minimum) || minimum < 0 || minimum > 0xffffffff ||
    !Number.isInteger(maximum) || maximum < minimum || maximum > 0xffffffff)
    throw new RangeError("Arbitrary.array: expected 0 <= minLength <= maxLength <= 4294967295 with integer lengths");
  return makeGen(minimum === 0 ? 0 : minimum * item.minCost, (st) => {
    const upper = Math.min(maximum, Math.max(minimum, st.size),
      item.minCost > 0 ? Math.floor(st.budget.remaining / item.minCost) : maximum);
    if (upper < minimum) return null;
    const length = randomLength(st, minimum, upper);
    const children = new Array<Gen>(length).fill(item);
    const order = length < 2 ? undefined : shuffle(st, [...children.keys()]);
    const itemState = st.size >= length ? st : withSize(st, length);
    const samples = generateProduct(children, itemState, order, length === 0 ? 0 : length * item.minCost);
    if (samples === undefined) return null;
    return arraySample(samples, { fixed: 0, optional: 0, repeat: length, tail: 0, minimum }, st.shrinks);
  });
};

/** One value of each, as a list. */
export const allGen = (members: ReadonlyArray<Gen>): Gen => {
  let minCost = 0;
  for (const m of members) minCost += m.minCost;
  const indexes = [...members.keys()];
  return makeGen(minCost, (st) => {
    const order = indexes.length < 2 ? undefined : shuffle(st, indexes);
    const samples = generateProduct(members, st, order, minCost);
    return samples === undefined ? null : productSample(samples, (values) => values);
  });
};

type Dependent = (value: unknown) => Gen;

const flatMapSourcePull = (source: Pull, f: Dependent, checkpoint: GenState, residual: number): Pull => {
  const stack = [source];
  return () => {
    for (let top = stack.at(-1); top !== undefined; top = stack.at(-1)) {
      const s = top();
      if (s === undefined) {
        stack.pop();
        continue;
      }
      if (s === null) return null;
      const target = f(s.value);
      const t = target.generate(withRandom(checkpoint, checkpoint.random.clone(), residual + target.minCost));
      if (t === null) {
        if (s.shrinks !== undefined) stack.push(s.shrinks);
        return null;
      }
      return flatMapSample(s, t, f, checkpoint, residual);
    }
    return undefined;
  };
};

// the source shrinks first; the dependent value is drawn again from where the source left the generator
const flatMapSample = (source: Sample, target: Sample, f: Dependent, checkpoint: GenState, residual: number): Sample => {
  if (source.shrinks === undefined) return target;
  const sources = flatMapSourcePull(source.shrinks, f, checkpoint, residual);
  return sampleOf(target.value, target.shrinks === undefined ? sources : concatPulls([sources, target.shrinks]));
};

export const flatMapGen = (self: Gen, f: Dependent): Gen => makeGen(self.minCost, (st) => {
  if (!st.shrinks) {
    const source = self.generate(st);
    if (source === null) return null;
    const target = f(source.value);
    const residual = st.budget.remaining;
    st.budget.remaining = residual + target.minCost;
    const s = target.generate(st);
    st.budget.remaining = Math.min(residual, st.budget.remaining);
    return s;
  }
  const sourceState = withRandom(st, st.random.clone(), st.budget.remaining);
  const source = self.generate(sourceState);
  if (source === null) {
    st.random.copyFrom(sourceState.random);
    st.budget.remaining = sourceState.budget.remaining;
    return null;
  }
  const target = f(source.value);
  const residual = sourceState.budget.remaining;
  // the generator's state where the source left it, which each dependent draw starts from
  const checkpoint = withRandom(st, sourceState.random.clone(), residual);
  const targetState = withRandom(st, checkpoint.random.clone(), residual + target.minCost);
  const s = target.generate(targetState);
  st.random.copyFrom(targetState.random);
  st.budget.remaining = Math.min(residual, targetState.budget.remaining);
  return s === null ? null : flatMapSample(source, s, f, checkpoint, residual);
});

/** An attempt's value: its generator, size and bias. */
const generateAttempt = (g: Gen, seed: Seed, attempt: number, size: number, shrinks: boolean): Sample | null =>
  g.generate({
    size, shrinks, biasFactor: 2 + Math.floor(Math.log10(attempt + 1)),
    random: attemptRandom(seed, attempt), budget: { remaining: g.minCost + size },
  });

/** A count of the options, a non-negative safe integer, `fallback` when absent. */
export const natural = (value: number | undefined, fallback: number, label: string): number => {
  const out = value ?? fallback;
  if (!Number.isSafeInteger(out) || out < 0) throw new Error(`${label} must be a non-negative safe integer`);
  return out;
};

const positive = (value: number | undefined, fallback: number, label: string): number => {
  const out = natural(value, fallback, label);
  if (out === 0) throw new Error(`${label} must be greater than zero`);
  return out;
};

/** The seed of the options, or one of the system's randomness. */
const seedOf = (seed: number | undefined): number => seed ?? Math.floor(Math.random() * 0x100000000) - 0x80000000;

export interface SampleOptions {
  readonly count: number | undefined;
  readonly size: number | undefined;
  readonly maxDiscards: number | undefined;
  readonly seed: number | undefined;
}

/** Values of attempts without shrinks; how far it got when the discards ran out. */
export const sample = (g: Gen, options: SampleOptions):
  { readonly values: Array<unknown> } | { readonly generated: number; readonly discards: number; readonly seed: number } => {
  const count = natural(options.count, 10, "count");
  const size = natural(options.size, 10, "size");
  const maxDiscards = natural(options.maxDiscards, Math.max(100, count * 10), "maxDiscards");
  const seed = seedOf(options.seed);
  const hashed = hashSeed(seed);
  const values: Array<unknown> = [];
  let discards = 0;
  for (let attempt = 0; values.length < count; attempt++) {
    const s = generateAttempt(g, hashed, attempt, size, false);
    if (s !== null) values.push(s.value);
    else if (++discards > maxDiscards) return { generated: values.length, discards, seed };
  }
  return { values };
};

/** Why a property failed. */
export type PropertyFailure = { readonly _tag: "ReturnedFalse" } | { readonly _tag: "PropertyError"; readonly error: unknown };
const returnedFalse: PropertyFailure = { _tag: "ReturnedFalse" };

/** The property's failure, `undefined` when it passed; a defect or an interruption of it ends the check. */
function* evaluate(property: (value: unknown) => unknown, value: unknown): Run {
  const exit = asExit(yield caught(() => property(value)));
  if (exit.cause === undefined) return exit.value === true ? undefined : returnedFalse;
  const reasons = exit.cause.reasons;
  if (!reasons.every(isFail)) return raise(fromReasons(reasons.filter((r) => !isFail(r))));
  if (unwinds(exit.cause)) return raise(exit.cause);
  const first = reasons.find(isFail);
  return { _tag: "PropertyError", error: first?.error };
}
const failureOf = (outcome: unknown): PropertyFailure | undefined =>
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- what evaluate returns
  outcome as PropertyFailure | undefined;

export type CheckResult =
  | { readonly _tag: "Passed"; readonly runs: number; readonly discards: number }
  | { readonly _tag: "Falsified"; readonly initialInput: unknown; readonly shrunkInput: unknown; readonly failure: PropertyFailure;
    readonly runs: number; readonly discards: number; readonly shrinks: number; readonly replay: string }
  | { readonly _tag: "Exhausted"; readonly runs: number; readonly discards: number; readonly seed: number }
  | { readonly _tag: "ReplayMismatch"; readonly reason: string };

export interface CheckOptions {
  readonly runs: number | undefined;
  readonly size: number | undefined;
  readonly maxDiscards: number | undefined;
  readonly maxShrinks: number | undefined;
  readonly seed: number | undefined;
  readonly replay: string | undefined;
}

/** The replay a token records, or the runs of the property, 100 unless the options say how many. */
export function* check(g: Gen, property: (value: unknown) => unknown, options: CheckOptions): Run {
  if (options.replay !== undefined) return yield replay(g, property, options.replay);
  const runsTarget = positive(options.runs, 100, "runs");
  const size = natural(options.size, 10, "size");
  const maxDiscards = natural(options.maxDiscards, Math.max(100, runsTarget * 10), "maxDiscards");
  const maxShrinks = natural(options.maxShrinks, 100, "maxShrinks");
  const seed = seedOf(options.seed);
  const hashed = hashSeed(seed);
  let runs = 0;
  let discards = 0;
  for (let attempt = 0; runs < runsTarget; attempt++) {
    const currentSize = runsTarget === 1 ? size : Math.round((runs * size) / (runsTarget - 1));
    const s = generateAttempt(g, hashed, attempt, currentSize, true);
    if (s === null) {
      if (++discards > maxDiscards) return { _tag: "Exhausted", runs, discards, seed } satisfies CheckResult;
      continue;
    }
    runs++;
    const failure = failureOf(yield evaluate(property, s.value));
    if (failure === undefined) continue;
    // the first-failing candidate at each step, at most `maxShrinks` candidates in all; the path is the index of each
    let current = s;
    let last = failure;
    let inspected = 0;
    const path: Array<number> = [];
    for (let found = true; found && inspected < maxShrinks;) {
      found = false;
      for (let index = 0; inspected < maxShrinks && current.shrinks !== undefined;) {
        const candidate = current.shrinks();
        if (candidate === undefined) break;
        inspected++;
        if (candidate === null) continue;
        const outcome = failureOf(yield evaluate(property, candidate.value));
        if (outcome?._tag === failure._tag) {
          current = candidate;
          last = outcome;
          path.push(index);
          found = true;
          break;
        }
        index++;
      }
    }
    const token = JSON.stringify([0, String(seed), attempt, currentSize, path, failure._tag]);
    return { _tag: "Falsified", initialInput: s.value, shrunkInput: current.value, failure: last, runs, discards,
      shrinks: path.length, replay: token } satisfies CheckResult;
  }
  return { _tag: "Passed", runs, discards } satisfies CheckResult;
}

const mismatch = (reason: string): CheckResult => ({ _tag: "ReplayMismatch", reason });

/** The attempt a replay token records, and the recorded path of its shrinks. */
function* replay(g: Gen, property: (value: unknown) => unknown, token: string): Run {
  const data: unknown = JSON.parse(token);
  if (!Array.isArray(data)) throw new SyntaxError("Invalid Arbitrary replay token");
  const [kind, seed, attempt, size, path, tag] = data;
  const s = generateAttempt(g, hashSeed(kind === 0 ? Number(seed) : String(seed)), Number(attempt), Number(size), true);
  if (s === null) return mismatch("AttemptDiscarded");
  const failure = failureOf(yield evaluate(property, s.value));
  if (failure === undefined) return mismatch("PropertyPassed");
  if (failure._tag !== tag) return mismatch("ShrinkPassed");
  let current = s;
  let last = failure;
  const steps: ReadonlyArray<unknown> = Array.isArray(path) ? path : [];
  for (const target of steps) {
    let selected: Sample | undefined;
    for (let index = 0; index <= Number(target);) {
      const candidate = current.shrinks?.();
      if (candidate === undefined) return mismatch("ShrinkPathUnavailable");
      if (candidate === null) continue;
      if (index === Number(target)) selected = candidate;
      index++;
    }
    if (selected === undefined) return mismatch("ShrinkPathUnavailable");
    const outcome = failureOf(yield evaluate(property, selected.value));
    if (outcome?._tag !== failure._tag) return mismatch("ShrinkPassed");
    current = selected;
    last = outcome;
  }
  return { _tag: "Falsified", initialInput: s.value, shrunkInput: current.value, failure: last, runs: 1, discards: 0,
    shrinks: steps.length, replay: token } satisfies CheckResult;
}
