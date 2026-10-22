// Strings, numbers, BigInt and the process. Checked integer operations
// return a `Result`: their failure is data, not a thrown exception.
import { err, none, ok, some } from "../data.js";

const writeStream = (stream, text) => void stream.write(text);

export const stdoutWrite = (text) => writeStream(process.stdout, text);

export const platformArgs = () => process.argv.slice(2);

export const platformEnv = (name) => {
  const value = process.env[name];
  return value === undefined ? none : some(value);
};

/** `Log.console`: info and debug to stdout, warnings and errors to stderr. */
export const logConsole = (level, message) => {
  const stream = level === "warn" || level === "error" ? process.stderr : process.stdout;
  writeStream(stream, `${level}: ${message}\n`);
};

export const stringSplit = (separator, s) => s.split(separator);
export const stringJoin = (separator, parts) => parts.join(separator);
export const stringContains = (part, s) => s.includes(part);
export const stringStartsWith = (prefix, s) => s.startsWith(prefix);
export const stringEndsWith = (suffix, s) => s.endsWith(suffix);
export const stringIndexOf = (part, s) => {
  const i = s.indexOf(part);
  return i < 0 ? none : some(i);
};
export const stringToUpper = (s) => s.toUpperCase();
export const stringToLower = (s) => s.toLowerCase();
export const stringSlice = (start, end, s) => s.slice(start, end);
export const stringReplaceAll = (pattern, replacement, s) => s.replaceAll(pattern, () => replacement);
export const stringRepeat = (n, s) => (Number.isInteger(n) && n >= 0 ? s.repeat(n) : "");
export const stringChars = (s) => Array.from(s);
export const stringFromNumber = (n) => (Object.is(n, -0) ? "-0" : String(n));

/** The number `s` spells in JS syntax; blank and malformed text are `None`. */
export const stringToNumber = (s) => {
  const text = s.trim();
  if (text === "") return none;
  const n = Number(text);
  return Number.isNaN(n) && text !== "NaN" ? none : some(n);
};

const ARITHMETIC = "Number.ArithmeticError.ArithmeticError";
const arithmetic = (message) => err({ _tag: ARITHMETIC, _0: message });

export const numberSafeInteger = (n) => (Number.isSafeInteger(n) ? some(n) : none);

/** A checked integer operation: operands and result must be safe integers. */
const checked = (name, f) => (a, b) => {
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

export const bitAnd = (a, b) => a & b;
export const bitOr = (a, b) => a | b;
export const bitXor = (a, b) => a ^ b;
export const bitNot = (a) => ~a;
export const shiftLeft = (a, b) => a << b;
export const shiftRight = (a, b) => a >> b;
export const shiftRightUnsigned = (a, b) => a >>> b;

export const bigintFromNumber = (n) => (Number.isInteger(n) ? some(BigInt(n)) : none);

export const bigintParse = (s) => {
  const text = s.trim();
  if (!/^[+-]?\d+$/.test(text)) return none;
  return some(BigInt(text));
};

export const bigintAbs = (n) => (n < 0n ? -n : n);

export const bigintDivide = (a, b) => (b === 0n ? arithmetic("division by zero") : ok(a / b));
export const bigintRemainder = (a, b) => (b === 0n ? arithmetic("division by zero") : ok(a % b));
