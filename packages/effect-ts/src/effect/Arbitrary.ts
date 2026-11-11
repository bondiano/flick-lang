// Effect's `Arbitrary` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: options
// records with `Option` fields, and the outcome of a property check as the
// variants of `Arbitrary.CheckResult`.
import { Arbitrary, Effect, Option } from "effect";
import type { Schema } from "effect";

import { tag } from "@flick-lang/runtime/tags";
import { nativeArbitraryValue } from "../bridge.ts";

export * from "effect/Arbitrary";

/** Values of a native schema keep Flick's plain data ABI through generation and shrinking. */
export const schema = <A>(value: Schema.Codec<A, unknown, unknown, unknown>): Arbitrary.Arbitrary<A> =>
  Arbitrary.map(Arbitrary.schema(value), (item) => nativeArbitraryValue(value, item));

const SAMPLE_ERROR = /*#__PURE__*/ tag("Arbitrary.SampleError.SampleError");
const RETURNED_FALSE = /*#__PURE__*/ tag("Arbitrary.PropertyFailure.ReturnedFalse");
const PROPERTY_ERROR = /*#__PURE__*/ tag("Arbitrary.PropertyFailure.PropertyError");
const PASSED = /*#__PURE__*/ tag("Arbitrary.CheckResult.Passed");
const FALSIFIED = /*#__PURE__*/ tag("Arbitrary.CheckResult.Falsified");
const EXHAUSTED = /*#__PURE__*/ tag("Arbitrary.CheckResult.Exhausted");
const REPLAY_MISMATCH = /*#__PURE__*/ tag("Arbitrary.CheckResult.ReplayMismatch");

/** `Arbitrary.ArrayOptions`. */
interface ArrayOptions {
  readonly minLength: Option.Option<number>;
  readonly maxLength: Option.Option<number>;
}

export const flickArray = <A>(options: ArrayOptions, self: Arbitrary.Arbitrary<A>): Arbitrary.Arbitrary<Array<A>> =>
  Arbitrary.array(self, {
    minLength: Option.getOrUndefined(options.minLength),
    maxLength: Option.getOrUndefined(options.maxLength),
  });

/** `Arbitrary.zip2`: `all` over two generators, whose values are a tuple. */
export const flickZip2 = <A, B>(
  first: Arbitrary.Arbitrary<A>,
  second: Arbitrary.Arbitrary<B>,
): Arbitrary.Arbitrary<[A, B]> => Arbitrary.all([first, second]);

/** `Arbitrary.zip3`: `all` over three generators, whose values are a tuple. */
export const flickZip3 = <A, B, C>(
  first: Arbitrary.Arbitrary<A>,
  second: Arbitrary.Arbitrary<B>,
  third: Arbitrary.Arbitrary<C>,
): Arbitrary.Arbitrary<[A, B, C]> => Arbitrary.all([first, second, third]);

/** `Arbitrary.zip4`: `all` over four generators, whose values are a tuple. */
export const flickZip4 = <A, B, C, D>(
  first: Arbitrary.Arbitrary<A>,
  second: Arbitrary.Arbitrary<B>,
  third: Arbitrary.Arbitrary<C>,
  fourth: Arbitrary.Arbitrary<D>,
): Arbitrary.Arbitrary<[A, B, C, D]> => Arbitrary.all([first, second, third, fourth]);

/** `Arbitrary.SchemaOptions a`. */
interface SchemaOptions<A> {
  readonly shrink: Option.Option<(value: A) => ReadonlyArray<A>>;
}

export const flickSchema = <A>(options: SchemaOptions<A>, codec: Schema.Codec<A, unknown, unknown, unknown>): Arbitrary.Arbitrary<A> =>
  Arbitrary.map(
    Arbitrary.schema(codec, { shrink: Option.getOrUndefined(options.shrink) }),
    (item) => nativeArbitraryValue(codec, item),
  );

/** `Arbitrary.SampleOptions`. */
interface SampleOptions {
  readonly count: Option.Option<number>;
  readonly size: Option.Option<number>;
  readonly maxDiscards: Option.Option<number>;
  readonly seed: Option.Option<number>;
}

/** `Arbitrary.SampleError`. */
interface SampleError {
  readonly _tag: typeof SAMPLE_ERROR;
  readonly _0: { readonly generated: number; readonly discards: number; readonly seed: string };
}

export const flickSampleEffect = <A>(
  options: SampleOptions,
  self: Arbitrary.Arbitrary<A>,
): Effect.Effect<ReadonlyArray<A>, SampleError> =>
  Effect.mapError(
    Arbitrary.sampleEffect(self, {
      count: Option.getOrUndefined(options.count),
      size: Option.getOrUndefined(options.size),
      maxDiscards: Option.getOrUndefined(options.maxDiscards),
      seed: Option.getOrUndefined(options.seed),
    }),
    (e): SampleError => ({
      _tag: SAMPLE_ERROR,
      _0: Object.freeze({ generated: e.generated, discards: e.discards, seed: String(e.seed) }),
    }),
  );

/** `Arbitrary.CheckOptions`. */
interface CheckOptions {
  readonly runs: Option.Option<number>;
  readonly size: Option.Option<number>;
  readonly maxDiscards: Option.Option<number>;
  readonly maxShrinks: Option.Option<number>;
  readonly seed: Option.Option<number>;
  readonly replay: Option.Option<string>;
}

/** `Arbitrary.PropertyFailure e`: `ReturnedFalse`, or `PropertyError` with the error in `_0`. */
interface PropertyFailure<E> {
  readonly _tag: string;
  readonly _0?: E;
}

interface Passed {
  readonly _tag: string;
  readonly _0: { readonly runs: number; readonly discards: number };
}

interface Falsified<A, E> {
  readonly _tag: string;
  readonly _0: {
    readonly initialInput: A;
    readonly shrunkInput: A;
    readonly failure: PropertyFailure<E>;
    readonly runs: number;
    readonly discards: number;
    readonly shrinks: number;
    readonly replay: string;
  };
}

interface Exhausted {
  readonly _tag: string;
  readonly _0: { readonly runs: number; readonly discards: number; readonly seed: string };
}

interface ReplayMismatch {
  readonly _tag: string;
  readonly _0: string;
}

/** `Arbitrary.CheckResult e a`. */
type CheckResult<A, E> = Passed | Falsified<A, E> | Exhausted | ReplayMismatch;

const isPassed = <A, E>(r: CheckResult<A, E>): r is Passed => r._tag === PASSED;
const isFalsified = <A, E>(r: CheckResult<A, E>): r is Falsified<A, E> => r._tag === FALSIFIED;
const isExhausted = <A, E>(r: CheckResult<A, E>): r is Exhausted => r._tag === EXHAUSTED;

const toFlick = <A, E>(r: Arbitrary.CheckResult<A, E>): CheckResult<A, E> => {
  switch (r._tag) {
    case "Passed":
      return { _tag: PASSED, _0: Object.freeze({ runs: r.runs, discards: r.discards }) };
    case "Falsified":
      return {
        _tag: FALSIFIED,
        _0: Object.freeze({
          initialInput: r.initialInput,
          shrunkInput: r.shrunkInput,
          failure:
            r.failure._tag === "ReturnedFalse" ? { _tag: RETURNED_FALSE } : { _tag: PROPERTY_ERROR, _0: r.failure.error },
          runs: r.runs,
          discards: r.discards,
          shrinks: r.shrinks,
          replay: r.replay,
        }),
      };
    case "Exhausted":
      return { _tag: EXHAUSTED, _0: Object.freeze({ runs: r.runs, discards: r.discards, seed: String(r.seed) }) };
    case "ReplayMismatch":
      return { _tag: REPLAY_MISMATCH, _0: r.reason };
    default:
      return r;
  }
};

const REASONS = ["AttemptDiscarded", "PropertyPassed", "ShrinkPathUnavailable", "ShrinkPassed"] as const;

const failureOf = <E>(f: PropertyFailure<E>): Arbitrary.PropertyFailure<E> =>
  f._tag === PROPERTY_ERROR && "_0" in f ? { _tag: "PropertyError", error: f._0 } : { _tag: "ReturnedFalse" };

const toEffect = (r: CheckResult<unknown, unknown>): Arbitrary.CheckResult<unknown, unknown> => {
  if (isPassed(r)) return { _tag: "Passed", runs: r._0.runs, discards: r._0.discards };
  if (isFalsified(r)) {
    const f = r._0;
    return {
      _tag: "Falsified",
      initialInput: f.initialInput,
      shrunkInput: f.shrunkInput,
      failure: failureOf(f.failure),
      runs: f.runs,
      discards: f.discards,
      shrinks: f.shrinks,
      replay: f.replay,
    };
  }
  if (isExhausted(r)) return { _tag: "Exhausted", runs: r._0.runs, discards: r._0.discards, seed: r._0.seed };
  const reason = REASONS.find((x) => x === r._0) ?? "AttemptDiscarded";
  return { _tag: "ReplayMismatch", reason };
};

export const flickCheckEffect = <A, E, R>(
  options: CheckOptions,
  property: (value: A) => Effect.Effect<boolean, E, R>,
  self: Arbitrary.Arbitrary<A>,
): Effect.Effect<CheckResult<A, E>, never, R> =>
  Effect.map(
    Arbitrary.checkEffect(self, property, {
      runs: Option.getOrUndefined(options.runs),
      size: Option.getOrUndefined(options.size),
      maxDiscards: Option.getOrUndefined(options.maxDiscards),
      maxShrinks: Option.getOrUndefined(options.maxShrinks),
      seed: Option.getOrUndefined(options.seed),
      replay: Option.getOrUndefined(options.replay),
    }),
    toFlick,
  );

/** `Arbitrary.format_check_failure`: the report of a failed check, `None` when it passed. */
export const flickFormatCheckFailure = (result: CheckResult<unknown, unknown>): Option.Option<string> =>
  Option.fromUndefinedOr(Arbitrary.formatCheckFailure(toEffect(result)));
