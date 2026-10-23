// Strings, numbers, BigInt and the process. Checked integer operations
// return a `Result`: their failure is data, not a thrown exception.
import type { Option, Result } from "effect";

import { type Tagged, err, none, ok, some } from "../data.ts";

const writeStream = (stream: NodeJS.WriteStream, text: string): void => void stream.write(text);

export const stdoutWrite = (text: string): void => writeStream(process.stdout, text);

export const platformArgs = (): Array<string> => process.argv.slice(2);

export const platformEnv = (name: string): Option.Option<string> => {
  const value = process.env[name];
  return value === undefined ? none : some(value);
};

/** `Log.console`: info and debug to stdout, warnings and errors to stderr. */
export const logConsole = (level: string, message: string): void => {
  const stream = level === "warn" || level === "error" ? process.stderr : process.stdout;
  writeStream(stream, `${level}: ${message}\n`);
};

export const stringSplit = (separator: string, s: string): Array<string> => s.split(separator);
export const stringJoin = (separator: string, parts: ReadonlyArray<string>): string => parts.join(separator);
export const stringContains = (part: string, s: string): boolean => s.includes(part);
export const stringStartsWith = (prefix: string, s: string): boolean => s.startsWith(prefix);
export const stringEndsWith = (suffix: string, s: string): boolean => s.endsWith(suffix);
export const stringIndexOf = (part: string, s: string): Option.Option<number> => {
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
export const stringToNumber = (s: string): Option.Option<number> => {
  const text = s.trim();
  if (text === "") return none;
  const n = Number(text);
  return Number.isNaN(n) && text !== "NaN" ? none : some(n);
};

const ARITHMETIC = "Number.ArithmeticError.ArithmeticError";

/** `Number.ArithmeticError`. */
export type ArithmeticError = Tagged<typeof ARITHMETIC, string>;

const arithmetic = (message: string): Result.Result<never, ArithmeticError> => err({ _tag: ARITHMETIC, _0: message });

export const numberSafeInteger = (n: number): Option.Option<number> => (Number.isSafeInteger(n) ? some(n) : none);

type Checked = (a: number, b: number) => Result.Result<number, ArithmeticError>;

/** A checked integer operation: operands and result must be safe integers. */
const checked =
  (name: string, f: (a: number, b: number) => number): Checked =>
  (a, b) => {
    if (!Number.isSafeInteger(a) || !Number.isSafeInteger(b)) {
      return arithmetic(`${name}: ${a} and ${b} must be safe integers`);
    }
    if ((name === "div_int" || name === "rem_int") && b === 0) {
      return arithmetic(`${name}: division by zero`);
    }
    const r = f(a, b);
    return Number.isSafeInteger(r) ? ok(r + 0) : arithmetic(`${name}: ${r} is not a safe integer`);
  };

export const intAdd = checked("add_int", (a, b) => a + b);
export const intSub = checked("sub_int", (a, b) => a - b);
export const intMul = checked("mul_int", (a, b) => a * b);
export const intDiv = checked("div_int", (a, b) => Math.trunc(a / b));
export const intRem = checked("rem_int", (a, b) => a % b);

export const bitAnd = (a: number, b: number): number => a & b;
export const bitOr = (a: number, b: number): number => a | b;
export const bitXor = (a: number, b: number): number => a ^ b;
export const bitNot = (a: number): number => ~a;
export const shiftLeft = (a: number, b: number): number => a << b;
export const shiftRight = (a: number, b: number): number => a >> b;
export const shiftRightUnsigned = (a: number, b: number): number => a >>> b;

export const bigintFromNumber = (n: number): Option.Option<bigint> => (Number.isInteger(n) ? some(BigInt(n)) : none);

export const bigintParse = (s: string): Option.Option<bigint> => {
  const text = s.trim();
  if (!/^[+-]?\d+$/.test(text)) return none;
  return some(BigInt(text));
};

export const bigintAbs = (n: bigint): bigint => (n < 0n ? -n : n);

export const bigintDivide = (a: bigint, b: bigint): Result.Result<bigint, ArithmeticError> =>
  b === 0n ? arithmetic("division by zero") : ok(a / b);
export const bigintRemainder = (a: bigint, b: bigint): Result.Result<bigint, ArithmeticError> =>
  b === 0n ? arithmetic("division by zero") : ok(a % b);
