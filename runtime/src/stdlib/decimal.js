// BigDecimal and Duration: native Effect values behind checked
// constructors. Division computes quotient and remainder on BigInt, with the
// rounding the caller names; there is no global precision.
import { BigDecimal, Duration } from "effect";

import { EQ, GT, LT, err, none, ok, some } from "../data.js";

const DECIMAL = "BigDecimal.DecimalError.DecimalError";
const decimalError = (message) => err({ _tag: DECIMAL, _0: message });

const ordering = (n) => (n < 0 ? LT : n > 0 ? GT : EQ);

const scaleOk = (scale) => Number.isSafeInteger(scale);

export const decimalFromString = (s) => {
  const parsed = BigDecimal.fromString(s.trim());
  return parsed._tag === "Some" ? ok(parsed.value) : decimalError(`not a decimal: ${JSON.stringify(s)}`);
};

export const decimalFromBigInt = (value, scale) =>
  scaleOk(scale) ? ok(BigDecimal.make(value, scale)) : decimalError(`scale ${scale} is not a safe integer`);

export const decimalFromInteger = (value) => BigDecimal.make(value, 0);

export const decimalFromNumber = (n) => {
  if (!Number.isFinite(n)) return decimalError(`${n} is not finite`);
  const parsed = BigDecimal.fromNumber(n);
  return parsed._tag === "Some" ? ok(parsed.value) : decimalError(`${n} has no decimal form`);
};

export const decimalToString = (d) => BigDecimal.format(d);
export const decimalNormalString = (d) => BigDecimal.format(BigDecimal.normalize(d));
export const decimalScale = (d) => d.scale;
export const decimalAdd = (a, b) => BigDecimal.sum(a, b);
export const decimalSubtract = (a, b) => BigDecimal.subtract(a, b);

export const decimalMultiply = (a, b) => {
  const scale = a.scale + b.scale;
  return scaleOk(scale)
    ? ok(BigDecimal.make(BigInt(a.value) * BigInt(b.value), scale))
    : decimalError(`the product's scale ${scale} is not a safe integer`);
};

export const decimalCompare = (a, b) => ordering(BigDecimal.Order(a, b));

const MODE = "BigDecimal.RoundingMode.";

const abs = (n) => (n < 0n ? -n : n);

/** `num / den` rounded to an integer by `mode`; `undefined` when `Exact` cannot. */
const roundQuotient = (num, den, mode) => {
  const q = num / den;
  const r = num % den;
  if (r === 0n) return q;
  const sign = (num < 0n) !== (den < 0n) ? -1n : 1n;
  const half = 2n * abs(r) - abs(den);
  switch (mode) {
    case `${MODE}TowardZero`:
      return q;
    case `${MODE}Floor`:
      return sign < 0n ? q - 1n : q;
    case `${MODE}Ceiling`:
      return sign > 0n ? q + 1n : q;
    case `${MODE}HalfAwayFromZero`:
      return half >= 0n ? q + sign : q;
    case `${MODE}HalfEven`:
      if (half > 0n) return q + sign;
      if (half < 0n) return q;
      return q % 2n === 0n ? q : q + sign;
    default:
      return undefined;
  }
};

/** `divide_to_scale scale mode a b`. */
export const decimalDivideToScale = (scale, mode, a, b) => {
  if (!scaleOk(scale)) return decimalError(`scale ${scale} is not a safe integer`);
  const av = BigInt(a.value);
  const bv = BigInt(b.value);
  if (bv === 0n) return decimalError("division by zero");
  // a / b = av / bv × 10^(b.scale - a.scale); the result has `scale` digits
  const k = b.scale + scale - a.scale;
  const num = k >= 0 ? av * 10n ** BigInt(k) : av;
  const den = k >= 0 ? bv : bv * 10n ** BigInt(-k);
  const q = roundQuotient(num, den, mode._tag);
  return q === undefined
    ? decimalError("the quotient needs rounding, and the mode is Exact")
    : ok(BigDecimal.make(q, scale));
};

const DURATION = "Duration.DurationError.DurationError";
const durationError = (message) => err({ _tag: DURATION, _0: message });

/** A duration of `n` units of `millisPer` milliseconds: finite and not negative. */
const checkedDuration = (unit, n, millisPer) => {
  if (Number.isNaN(n)) return durationError(`${unit}: NaN is not a duration`);
  if (n < 0) return durationError(`${unit}: ${n} is negative`);
  const millis = n * millisPer;
  if (!Number.isFinite(millis)) {
    return durationError(`${unit}: ${n} overflows; use Duration.infinity for an unbounded duration`);
  }
  return ok(Duration.millis(millis));
};

export const durationMillis = (n) => checkedDuration("millis", n, 1);
export const durationSeconds = (n) => checkedDuration("seconds", n, 1000);
export const durationMinutes = (n) => checkedDuration("minutes", n, 60_000);

export const durationNanos = (n) =>
  n < 0n ? durationError(`nanos: ${n} is negative`) : ok(Duration.nanos(n));

export const durationZero = Duration.zero;
export const durationInfinity = Duration.infinity;
export const durationIsFinite = (d) => Duration.isFinite(d);
export const durationToMillis = (d) => (Duration.isFinite(d) ? some(Duration.toMillis(d)) : none);
export const durationAdd = (a, b) => Duration.sum(a, b);
export const durationCompare = (a, b) => ordering(Duration.Order(a, b));
export const durationFormat = (d) => (Duration.isFinite(d) ? Duration.format(d) : "Infinity");
