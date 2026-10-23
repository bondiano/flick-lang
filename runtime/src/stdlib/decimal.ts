// BigDecimal and Duration: native Effect values behind checked
// constructors. Division computes quotient and remainder on BigInt, with the
// rounding the caller names; there is no global precision.
import { BigDecimal, Duration, Option, type Result } from "effect";

import { EQ, GT, LT, type Ordering, type Tagged, err, none, ok, some } from "../data.ts";
import { tag } from "../tags.ts";

const DECIMAL = tag("BigDecimal.DecimalError.DecimalError");

/** `BigDecimal.DecimalError`. */
export type DecimalError = Tagged<typeof DECIMAL, string>;

const decimalError = (message: string): Result.Result<never, DecimalError> => err({ _tag: DECIMAL, _0: message });

const ordering = (n: number): Ordering => (n < 0 ? LT : n > 0 ? GT : EQ);

const scaleOk = (scale: number): boolean => Number.isSafeInteger(scale);

type Decimal = BigDecimal.BigDecimal;

export const decimalFromString = (s: string): Result.Result<Decimal, DecimalError> => {
  const parsed = BigDecimal.fromString(s.trim());
  return Option.isSome(parsed) ? ok(parsed.value) : decimalError(`not a decimal: ${JSON.stringify(s)}`);
};

export const decimalFromBigInt = (value: bigint, scale: number): Result.Result<Decimal, DecimalError> =>
  scaleOk(scale) ? ok(BigDecimal.make(value, scale)) : decimalError(`scale ${scale} is not a safe integer`);

export const decimalFromInteger = (value: bigint): Decimal => BigDecimal.make(value, 0);

export const decimalFromNumber = (n: number): Result.Result<Decimal, DecimalError> => {
  if (!Number.isFinite(n)) return decimalError(`${n} is not finite`);
  const parsed = BigDecimal.fromNumber(n);
  return Option.isSome(parsed) ? ok(parsed.value) : decimalError(`${n} has no decimal form`);
};

export const decimalToString = (d: Decimal): string => BigDecimal.format(d);
export const decimalNormalString = (d: Decimal): string => BigDecimal.format(BigDecimal.normalize(d));
export const decimalScale = (d: Decimal): number => d.scale;
export const decimalAdd = (a: Decimal, b: Decimal): Decimal => BigDecimal.sum(a, b);
export const decimalSubtract = (a: Decimal, b: Decimal): Decimal => BigDecimal.subtract(a, b);

export const decimalMultiply = (a: Decimal, b: Decimal): Result.Result<Decimal, DecimalError> => {
  const scale = a.scale + b.scale;
  return scaleOk(scale)
    ? ok(BigDecimal.make(a.value * b.value, scale))
    : decimalError(`the product's scale ${scale} is not a safe integer`);
};

export const decimalCompare = (a: Decimal, b: Decimal): Ordering => ordering(BigDecimal.Order(a, b));

/** `BigDecimal.RoundingMode`. */
export type RoundingMode = Readonly<{ _tag: string }>;

const MODES = new Map(
  ["TowardZero", "Floor", "Ceiling", "HalfAwayFromZero", "HalfEven", "Exact"].map((m) => [
    tag(`BigDecimal.RoundingMode.${m}`),
    m,
  ]),
);

const abs = (n: bigint): bigint => (n < 0n ? -n : n);

/** `num / den` rounded to an integer by `mode`; `undefined` when `Exact` cannot. */
const roundQuotient = (num: bigint, den: bigint, mode: RoundingMode["_tag"]): bigint | undefined => {
  const q = num / den;
  const r = num % den;
  if (r === 0n) return q;
  const sign = num < 0n !== den < 0n ? -1n : 1n;
  const half = 2n * abs(r) - abs(den);
  switch (MODES.get(mode)) {
    case "TowardZero":
      return q;
    case "Floor":
      return sign < 0n ? q - 1n : q;
    case "Ceiling":
      return sign > 0n ? q + 1n : q;
    case "HalfAwayFromZero":
      return half >= 0n ? q + sign : q;
    case "HalfEven":
      if (half > 0n) return q + sign;
      if (half < 0n) return q;
      return q % 2n === 0n ? q : q + sign;
    default:
      // `Exact`: a remainder means the quotient cannot be exact
      return undefined;
  }
};

/** `divide_to_scale scale mode a b`. */
export const decimalDivideToScale = (
  scale: number,
  mode: RoundingMode,
  a: Decimal,
  b: Decimal,
): Result.Result<Decimal, DecimalError> => {
  if (!scaleOk(scale)) return decimalError(`scale ${scale} is not a safe integer`);
  if (b.value === 0n) return decimalError("division by zero");
  // a / b = a.value / b.value × 10^(b.scale - a.scale); the result has `scale` digits
  const k = b.scale + scale - a.scale;
  const num = k >= 0 ? a.value * 10n ** BigInt(k) : a.value;
  const den = k >= 0 ? b.value : b.value * 10n ** BigInt(-k);
  const q = roundQuotient(num, den, mode._tag);
  return q === undefined
    ? decimalError("the quotient needs rounding, and the mode is Exact")
    : ok(BigDecimal.make(q, scale));
};

const DURATION = tag("Duration.DurationError.DurationError");

/** `Duration.DurationError`. */
export type DurationError = Tagged<typeof DURATION, string>;

export const durationError = (message: string): Result.Result<never, DurationError> => err({ _tag: DURATION, _0: message });

/** A duration of `n` units of `millisPer` milliseconds: finite and not negative. */
export const checkedDuration = (unit: string, n: number, millisPer: number): Result.Result<Duration.Duration, DurationError> => {
  if (Number.isNaN(n)) return durationError(`${unit}: NaN is not a duration`);
  if (n < 0) return durationError(`${unit}: ${n} is negative`);
  const millis = n * millisPer;
  if (!Number.isFinite(millis)) {
    return durationError(`${unit}: ${n} overflows; use Duration.infinity for an unbounded duration`);
  }
  return ok(Duration.millis(millis));
};

export const durationMillis = (n: number): Result.Result<Duration.Duration, DurationError> => checkedDuration("millis", n, 1);
export const durationSeconds = (n: number): Result.Result<Duration.Duration, DurationError> =>
  checkedDuration("seconds", n, 1000);
export const durationMinutes = (n: number): Result.Result<Duration.Duration, DurationError> =>
  checkedDuration("minutes", n, 60_000);

export const durationNanos = (n: bigint): Result.Result<Duration.Duration, DurationError> =>
  n < 0n ? durationError(`nanos: ${n} is negative`) : ok(Duration.nanos(n));

export const durationZero: Duration.Duration = Duration.zero;
export const durationInfinity: Duration.Duration = Duration.infinity;
export const durationIsFinite = (d: Duration.Duration): boolean => Duration.isFinite(d);
export const durationToMillis = (d: Duration.Duration): Option.Option<number> =>
  Duration.isFinite(d) ? some(Duration.toMillis(d)) : none;
export const durationAdd = (a: Duration.Duration, b: Duration.Duration): Duration.Duration => Duration.sum(a, b);
export const durationCompare = (a: Duration.Duration, b: Duration.Duration): Ordering => ordering(Duration.Order(a, b));
export const durationFormat = (d: Duration.Duration): string => (Duration.isFinite(d) ? Duration.format(d) : "Infinity");
