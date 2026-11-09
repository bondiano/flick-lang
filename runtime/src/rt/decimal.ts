// Exact base-10 decimals: coefficient × 10^-scale. The checked operations
// retain Flick's DecimalError contract without importing Effect BigDecimal.
import { EQ, GT, LT, err, ok, type Ordering, type Result } from "../core/data.ts";
import { constructorName, tag } from "../tags.ts";

export interface Decimal { readonly value: bigint; readonly scale: number }
export interface DecimalError { readonly _tag: string; readonly _0: string }
const DECIMAL_ERROR = tag("BigDecimal.DecimalError.DecimalError");
export const decimalError = (message: string): Result<never, DecimalError> => err({ _tag: DECIMAL_ERROR, _0: message });
export const decimal = (value: bigint, scale: number): Decimal => ({ value, scale });
export const scaleOk = (scale: number): boolean => Number.isSafeInteger(scale);
export const normalize = (number: Decimal): Decimal => {
  if (number.value === 0n) return decimal(0n, 0);
  let { value, scale } = number;
  while (value % 10n === 0n) { value /= 10n; scale--; }
  return decimal(value, scale);
};
export const decimalFromString = (text: string): Result<Decimal, DecimalError> => {
  const source = text.trim();
  if (source === "") return ok(decimal(0n, 0));
  const match = /^([+-]?\d*)(?:\.(\d*))?(?:[eE]([+-]?\d+))?$/.exec(source);
  if (!match || !(match[1] ?? "").replace(/[+-]/, "") && !(match[2] ?? "")) {
    return decimalError(`not a decimal: ${JSON.stringify(text)}`);
  }
  const integer = match[1] ?? "";
  const fraction = match[2] ?? "";
  const exponent = Number(match[3] ?? 0);
  const scale = fraction.length - exponent;
  if (!scaleOk(exponent) || !scaleOk(scale)) return decimalError(`not a decimal: ${JSON.stringify(text)}`);
  try { return ok(decimal(BigInt(`${integer}${fraction}`), scale)); }
  catch { return decimalError(`not a decimal: ${JSON.stringify(text)}`); }
};
export const decimalFromBigInt = (value: bigint, scale: number): Result<Decimal, DecimalError> =>
  scaleOk(scale) ? ok(decimal(value, scale)) : decimalError(`scale ${scale} is not a safe integer`);
export const decimalFromInteger = (value: bigint): Decimal => decimal(value, 0);
export const decimalFromNumber = (value: number): Result<Decimal, DecimalError> =>
  Number.isFinite(value) ? decimalFromString(String(value)) : decimalError(`${value} is not finite`);
export const decimalToExponential = (number: Decimal): string => {
  if (number.value === 0n) return "0e+0";
  const normal = normalize(number);
  const digits = String(normal.value);
  const point = normal.value < 0n ? 2 : 1;
  const head = digits.slice(0, point);
  const tail = digits.slice(point);
  const exponent = tail.length - normal.scale;
  return `${head}${tail ? `.${tail}` : ""}e${exponent >= 0 ? "+" : ""}${exponent}`;
};
export const decimalToString = (number: Decimal): string => {
  const normal = normalize(number);
  if (Math.abs(normal.scale) >= 16) return decimalToExponential(normal);
  const negative = normal.value < 0n;
  const absolute = String(negative ? -normal.value : normal.value);
  const digits = normal.scale > 0 ? absolute.padStart(normal.scale + 1, "0") : absolute.padEnd(absolute.length - normal.scale, "0");
  const point = digits.length - normal.scale;
  const result = normal.scale > 0 ? `${digits.slice(0, point)}.${digits.slice(point)}` : digits;
  return negative ? `-${result}` : result;
};
export const decimalNormalString = (number: Decimal): string => decimalToString(normalize(number));
export const decimalScale = (number: Decimal): number => number.scale;
const align = (a: Decimal, b: Decimal): readonly [bigint, bigint, number] => {
  const scale = Math.max(a.scale, b.scale);
  return [a.value * 10n ** BigInt(scale - a.scale), b.value * 10n ** BigInt(scale - b.scale), scale];
};
export const decimalAdd = (a: Decimal, b: Decimal): Decimal => {
  const [x, y, scale] = align(a, b);
  return decimal(x + y, scale);
};
export const decimalSubtract = (a: Decimal, b: Decimal): Decimal => {
  const [x, y, scale] = align(a, b);
  return decimal(x - y, scale);
};
export const decimalMultiply = (a: Decimal, b: Decimal): Result<Decimal, DecimalError> => {
  const scale = a.scale + b.scale;
  return scaleOk(scale) ? ok(decimal(a.value * b.value, scale)) : decimalError(`the product's scale ${scale} is not a safe integer`);
};
export const decimalCompare = (a: Decimal, b: Decimal): Ordering => {
  const [x, y] = align(a, b);
  return x < y ? LT : x > y ? GT : EQ;
};

export type RoundingMode = Readonly<{ _tag: string }>;
const absolute = (value: bigint): bigint => value < 0n ? -value : value;
/** Integer quotient, rounded by Flick's named mode; undefined for inexact Exact. */
export const roundQuotient = (numerator: bigint, denominator: bigint, mode: RoundingMode): bigint | undefined => {
  const quotient = numerator / denominator;
  const remainder = numerator % denominator;
  if (remainder === 0n) return quotient;
  const sign = numerator < 0n !== denominator < 0n ? -1n : 1n;
  const half = 2n * absolute(remainder) - absolute(denominator);
  switch (constructorName(mode._tag)) {
    case "TowardZero": return quotient;
    case "AwayFromZero": return quotient + sign;
    case "Floor": return sign < 0n ? quotient - 1n : quotient;
    case "Ceiling": return sign > 0n ? quotient + 1n : quotient;
    case "HalfAwayFromZero": return half >= 0n ? quotient + sign : quotient;
    case "HalfTowardZero": return half > 0n ? quotient + sign : quotient;
    case "HalfFloor": return half > 0n || (half === 0n && sign < 0n) ? quotient + sign : quotient;
    case "HalfCeiling": return half > 0n || (half === 0n && sign > 0n) ? quotient + sign : quotient;
    case "HalfEven": return half > 0n || (half === 0n && quotient % 2n !== 0n) ? quotient + sign : quotient;
    case "HalfOdd": return half > 0n || (half === 0n && quotient % 2n === 0n) ? quotient + sign : quotient;
    default: return undefined;
  }
};
export const decimalDivideToScale = (
  scale: number, mode: RoundingMode, a: Decimal, b: Decimal,
): Result<Decimal, DecimalError> => {
  if (!scaleOk(scale)) return decimalError(`scale ${scale} is not a safe integer`);
  if (b.value === 0n) return decimalError("division by zero");
  const shift = b.scale + scale - a.scale;
  const numerator = shift >= 0 ? a.value * 10n ** BigInt(shift) : a.value;
  const denominator = shift >= 0 ? b.value : b.value * 10n ** BigInt(-shift);
  const quotient = roundQuotient(numerator, denominator, mode);
  return quotient === undefined
    ? decimalError("the quotient needs rounding, and the mode is Exact")
    : ok(decimal(quotient, scale));
};
