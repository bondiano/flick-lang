// Pure BigDecimal operations over Flick's coefficient-and-scale representation.
import { EQ, GT, LT, none, ok, some, type Option, type Ordering, type Result } from "../../core/data.ts";
import {
  decimal, decimalAdd, decimalCompare, decimalError, decimalMultiply, decimalSubtract,
  decimalToExponential, normalize, roundQuotient, scaleOk, type Decimal, type DecimalError, type RoundingMode,
} from "../decimal.ts";
import { tag } from "../../tags.ts";

export { normalize };
const sign = (value: Decimal): Ordering => value.value < 0n ? LT : value.value > 0n ? GT : EQ;
export const flickSign = sign;
export const abs = (value: Decimal): Decimal => decimal(value.value < 0n ? -value.value : value.value, value.scale);
export const negate = (value: Decimal): Decimal => decimal(-value.value, value.scale);
export const toExponential = decimalToExponential;
export const isZero = (value: Decimal): boolean => value.value === 0n;
export const isNegative = (value: Decimal): boolean => value.value < 0n;
export const isPositive = (value: Decimal): boolean => value.value > 0n;
export const isInteger = (value: Decimal): boolean =>
  value.scale <= 0 || value.value % 10n ** BigInt(value.scale) === 0n;
export const sumAll = (values: Iterable<Decimal>): Decimal => {
  let result = decimal(0n, 0);
  for (const value of values) result = decimalAdd(result, value);
  return result;
};
export const multiplyAll = (values: Iterable<Decimal>): Decimal => {
  let result = decimal(1n, 0);
  for (const value of values) {
    const product = decimalMultiply(result, value);
    if (product._tag === "Failure") throw new RangeError(product.failure._0);
    result = product.success;
  }
  return result;
};
const compare = (a: Decimal, b: Decimal): number => {
  const order = decimalCompare(a, b)._tag;
  return order === LT._tag ? -1 : order === GT._tag ? 1 : 0;
};
export const min = (that: Decimal) => (self: Decimal): Decimal => compare(self, that) <= 0 ? self : that;
export const max = (that: Decimal) => (self: Decimal): Decimal => compare(self, that) >= 0 ? self : that;
export const isLessThan = (that: Decimal) => (self: Decimal): boolean => compare(self, that) < 0;
export const isLessThanOrEqualTo = (that: Decimal) => (self: Decimal): boolean => compare(self, that) <= 0;
export const isGreaterThan = (that: Decimal) => (self: Decimal): boolean => compare(self, that) > 0;
export const isGreaterThanOrEqualTo = (that: Decimal) => (self: Decimal): boolean => compare(self, that) >= 0;
interface Bounds { readonly minimum: Decimal; readonly maximum: Decimal }
export const between = (bounds: Bounds) => (self: Decimal): boolean =>
  compare(self, bounds.minimum) >= 0 && compare(self, bounds.maximum) <= 0;
export const clamp = (bounds: Bounds) => (self: Decimal): Decimal =>
  compare(self, bounds.minimum) < 0 ? bounds.minimum : compare(self, bounds.maximum) > 0 ? bounds.maximum : self;
const scaled = (value: Decimal, target: number): Decimal => {
  const difference = target - value.scale;
  return decimal(difference >= 0 ? value.value * 10n ** BigInt(difference) : value.value / 10n ** BigInt(-difference), target);
};
export const flickScale = (scale: number, value: Decimal): Result<Decimal, DecimalError> =>
  scaleOk(scale) ? ok(scaled(value, scale)) : decimalError(`scale ${scale} is not a safe integer`);
const rounding = (scale: number, mode: RoundingMode, value: Decimal, exactMessage: string): Result<Decimal, DecimalError> => {
  if (!scaleOk(scale)) return decimalError(`scale ${scale} is not a safe integer`);
  if (scale >= value.scale) return ok(scaled(value, scale));
  const divisor = 10n ** BigInt(value.scale - scale);
  const result = roundQuotient(value.value, divisor, mode);
  return result === undefined ? decimalError(exactMessage) : ok(decimal(result, scale));
};
const mode = (name: string): RoundingMode => ({ _tag: tag(`BigDecimal.RoundingMode.${name}`) });
export const flickRound = (scale: number, roundingMode: RoundingMode, value: Decimal): Result<Decimal, DecimalError> =>
  rounding(scale, roundingMode, value, "rounding is needed, and the mode is Exact");
export const flickTruncate = (scale: number, value: Decimal): Result<Decimal, DecimalError> =>
  rounding(scale, mode("TowardZero"), value, "");
export const flickCeil = (scale: number, value: Decimal): Result<Decimal, DecimalError> =>
  rounding(scale, mode("Ceiling"), value, "");
export const flickFloor = (scale: number, value: Decimal): Result<Decimal, DecimalError> =>
  rounding(scale, mode("Floor"), value, "");
const dividePrecision = (self: Decimal, that: Decimal): Decimal => {
  const negative = self.value < 0n !== that.value < 0n;
  let numerator = self.value < 0n ? -self.value : self.value;
  const denominator = that.value < 0n ? -that.value : that.value;
  let scale = self.scale - that.scale;
  while (numerator < denominator) { numerator *= 10n; scale++; }
  let quotient = numerator / denominator;
  let remainder = numerator % denominator;
  let count = String(quotient).length;
  while (remainder !== 0n && count < 100) {
    remainder *= 10n;
    quotient = quotient * 10n + remainder / denominator;
    remainder %= denominator;
    count++;
    scale++;
  }
  if (remainder * 10n / denominator >= 5n) quotient++;
  return decimal(negative ? -quotient : quotient, scale);
};
export const divide = (that: Decimal) => (self: Decimal): Option<Decimal> =>
  that.value === 0n ? none : self.value === 0n ? some(decimal(0n, 0)) : some(dividePrecision(self, that));
export const remainder = (that: Decimal) => (self: Decimal): Option<Decimal> => {
  if (that.value === 0n) return none;
  const target = Math.max(self.scale, that.scale);
  return some(decimal(scaled(self, target).value % scaled(that, target).value, target));
};
export const subtract = decimalSubtract;
