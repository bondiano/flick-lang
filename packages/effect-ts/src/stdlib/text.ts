// Strings, numbers, BigInt and the process. Checked integer operations
// return a `Result`: their failure is data, not a thrown exception.
import { BigInt as BI, type Option, type Result } from "effect";

import { type Tagged, err, none, ok, some } from "../data.ts";
import { args, env, writeStderr, writeStdout } from "../host.ts";
import { tag } from "@flick-lang/runtime/tags";

export const stdoutWrite = (text: string): void => writeStdout(text);

export const platformArgs = (): Array<string> => args();

export const platformEnv = (name: string): Option.Option<string> => {
  const value = env(name);
  return value === undefined ? none : some(value);
};

/** `Log.console`: info and debug to stdout, warnings and errors to stderr. */
export const logConsole = (level: string, message: string): void => {
  const write = level === "warn" || level === "error" ? writeStderr : writeStdout;
  write(`${level}: ${message}\n`);
};

export {
  stringSplit,
  stringJoin,
  stringContains,
  stringStartsWith,
  stringEndsWith,
  stringToUpper,
  stringToLower,
  stringSlice,
  stringReplaceAll,
  stringRepeat,
  stringChars,
  stringFromNumber,
  divInt,
  remInt,
  powInt,
  divBigInt,
  remBigInt,
  intUnsafeDiv,
  intUnsafeRem,
  intTruncate,
  intAbs,
  bitAnd,
  bitOr,
  bitXor,
  bitNot,
  shiftLeft,
  shiftRight,
  shiftRightUnsigned,
  shiftRightUnsignedInt,
  bigintAbs,
} from "@flick-lang/runtime/core/text";

// Option- and Result-making helpers: Effect's data here, plain data in `core/text.ts`
export const stringIndexOf = (part: string, s: string): Option.Option<number> => {
  const i = s.indexOf(part);
  return i < 0 ? none : some(i);
};

/** The number `s` spells in JS syntax; blank and malformed text are `None`. */
export const stringToNumber = (s: string): Option.Option<number> => {
  const text = s.trim();
  if (text === "") return none;
  const n = Number(text);
  return Number.isNaN(n) && text !== "NaN" ? none : some(n);
};

const ARITHMETIC = /*#__PURE__*/ tag("Int.ArithmeticError.ArithmeticError");

/** `Int.ArithmeticError`. */
export type ArithmeticError = Tagged<typeof ARITHMETIC, string>;

const arithmetic = (message: string): Result.Result<never, ArithmeticError> => err({ _tag: ARITHMETIC, _0: message });

export const intFromFloat = (n: number): Option.Option<number> => ((n | 0) === n ? some(n + 0) : none);

/** The `Int` the decimal digits spell, with an optional sign. */
export const intParse = (s: string): Option.Option<number> => {
  const text = s.trim();
  if (!/^[+-]?\d+$/.test(text)) return none;
  const n = Number(text);
  return (n | 0) === n ? some(n + 0) : none;
};

export const numberSafeInteger = (n: number): Option.Option<number> => (Number.isSafeInteger(n) ? some(n) : none);

type Checked = (a: number, b: number) => Result.Result<number, ArithmeticError>;

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

export const bigintFromNumber = (n: number): Option.Option<bigint> => (Number.isInteger(n) ? some(BigInt(n)) : none);

export const bigintParse = (s: string): Option.Option<bigint> => {
  const text = s.trim();
  if (!/^[+-]?\d+$/.test(text)) return none;
  return some(BigInt(text));
};

/** `BigInt.divide`: Effect's `BigInt.divide`, with the error of a zero divisor. */
export const bigintDivide = (a: bigint, b: bigint): Result.Result<bigint, ArithmeticError> => {
  const q = BI.divide(a, b);
  return q._tag === "Some" ? ok(q.value) : arithmetic("division by zero");
};
/** `BigInt.remainder`: Effect's `BigInt.remainder`, checked for a zero divisor. */
export const bigintRemainder = (a: bigint, b: bigint): Result.Result<bigint, ArithmeticError> =>
  b === 0n ? arithmetic("division by zero") : ok(BI.remainder(a, b));
