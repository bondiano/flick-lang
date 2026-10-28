// Strings, numbers and BigInt without Effect. Checked integer operations
// return a `Result`: their failure is data, not a thrown exception.
import { type Option, type Result, type Tagged, err, none, ok, some } from "./data.ts";
import { tag } from "../tags.ts";

export const stringSplit = (separator: string, s: string): Array<string> => s.split(separator);
export const stringJoin = (separator: string, parts: ReadonlyArray<string>): string => parts.join(separator);
export const stringContains = (part: string, s: string): boolean => s.includes(part);
export const stringStartsWith = (prefix: string, s: string): boolean => s.startsWith(prefix);
export const stringEndsWith = (suffix: string, s: string): boolean => s.endsWith(suffix);
export const stringIndexOf = (part: string, s: string): Option<number> => {
  const i = s.indexOf(part);
  return i < 0 ? none : some(i);
};
export const stringToUpper = (s: string): string => s.toUpperCase();
export const stringToLower = (s: string): string => s.toLowerCase();
export const stringSlice = (start: number, end: number, s: string): string => s.slice(start, end);
export const stringReplaceAll = (pattern: string, replacement: string, s: string): string =>
  s.replaceAll(pattern, () => replacement);
export const stringRepeat = (n: number, s: string): string => (Number.isInteger(n) && n >= 0 ? s.repeat(n) : "");
export const stringChars = (s: string): Array<string> => Array.from(s);
export const stringFromNumber = (n: number): string => (Object.is(n, -0) ? "-0" : String(n));

/** The number `s` spells in JS syntax; blank and malformed text are `None`. */
export const stringToNumber = (s: string): Option<number> => {
  const text = s.trim();
  if (text === "") return none;
  const n = Number(text);
  return Number.isNaN(n) && text !== "NaN" ? none : some(n);
};

const ARITHMETIC = /*#__PURE__*/ tag("Int.ArithmeticError.ArithmeticError");

/** `Int.ArithmeticError`. */
export type ArithmeticError = Tagged<typeof ARITHMETIC, string>;

const arithmetic = (message: string): Result<never, ArithmeticError> => err({ _tag: ARITHMETIC, _0: message });

export const intFromFloat = (n: number): Option<number> => ((n | 0) === n ? some(n + 0) : none);

/** The `Int` the decimal digits spell, with an optional sign. */
export const intParse = (s: string): Option<number> => {
  const text = s.trim();
  if (!/^[+-]?\d+$/.test(text)) return none;
  const n = Number(text);
  return (n | 0) === n ? some(n + 0) : none;
};

export const numberSafeInteger = (n: number): Option<number> => (Number.isSafeInteger(n) ? some(n) : none);

type Checked = (a: number, b: number) => Result<number, ArithmeticError>;

/** A checked `Int` operation: the result must be an `Int`. */
const checked =
  (name: string, f: (a: number, b: number) => number): Checked =>
  (a, b) => {
    if ((name === "checked_div" || name === "checked_rem") && b === 0) {
      return arithmetic(`Int.${name}: division by zero`);
    }
    const r = f(a, b);
    return (r | 0) === r ? ok(r + 0) : arithmetic(`Int.${name}: ${r} does not fit in Int`);
  };

export const intCheckedAdd = /*#__PURE__*/ checked("checked_add", (a, b) => a + b);
export const intCheckedSub = /*#__PURE__*/ checked("checked_sub", (a, b) => a - b);
export const intCheckedMul = /*#__PURE__*/ checked("checked_mul", (a, b) => a * b);
export const intCheckedDiv = /*#__PURE__*/ checked("checked_div", (a, b) => Math.trunc(a / b));
export const intCheckedRem = /*#__PURE__*/ checked("checked_rem", (a, b) => a % b);

// `Int` arithmetic: operators wrap around and divide by zero to zero

/** `a / b` on `Int`, truncated. */
export const divInt = (a: number, b: number): number => (b === 0 ? 0 : (a / b) | 0);
export const remInt = (a: number, b: number): number => (b === 0 ? 0 : a % b | 0);

/** `a ** b` on `Int`; a negative power is truncated like a division. */
export const powInt = (a: number, b: number): number => {
  if (b < 0) return a === 1 ? 1 : a === -1 ? (b % 2 === 0 ? 1 : -1) : 0;
  let result = 1;
  for (let base = a, e = b; e > 0; e >>>= 1) {
    if (e & 1) result = Math.imul(result, base);
    base = Math.imul(base, base);
  }
  return result;
};

export const divBigInt = (a: bigint, b: bigint): bigint => (b === 0n ? 0n : a / b);
export const remBigInt = (a: bigint, b: bigint): bigint => (b === 0n ? 0n : a % b);

export const intUnsafeDiv = (a: number, b: number): number => {
  if (b === 0) throw new RangeError("Int.unsafe_div: division by zero");
  return (a / b) | 0;
};
export const intUnsafeRem = (a: number, b: number): number => {
  if (b === 0) throw new RangeError("Int.unsafe_rem: division by zero");
  return a % b | 0;
};

/** `n` with its fraction dropped, wrapped into `Int`; NaN and the infinities are 0. */
export const intTruncate = (n: number): number => n | 0;
export const intAbs = (n: number): number => Math.abs(n) | 0;

export const bitAnd = (a: number, b: number): number => a & b;
export const bitOr = (a: number, b: number): number => a | b;
export const bitXor = (a: number, b: number): number => a ^ b;
export const bitNot = (a: number): number => ~a;
export const shiftLeft = (a: number, b: number): number => a << b;
export const shiftRight = (a: number, b: number): number => a >> b;
export const shiftRightUnsigned = (a: number, b: number): number => a >>> b;
export const shiftRightUnsignedInt = (a: number, b: number): number => (a >>> b) | 0;

export const bigintFromNumber = (n: number): Option<bigint> => (Number.isInteger(n) ? some(BigInt(n)) : none);

export const bigintParse = (s: string): Option<bigint> => {
  const text = s.trim();
  if (!/^[+-]?\d+$/.test(text)) return none;
  return some(BigInt(text));
};

export const bigintAbs = (n: bigint): bigint => (n < 0n ? -n : n);

export const numberStrictEqual = (a: number, b: number): boolean => a === b;
export const numberSameValue = (a: number, b: number): boolean => Object.is(a, b);
