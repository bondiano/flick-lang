// Algebraic reducers and combiners of exact integers.
import { EQ, GT, LT, none, some, type Option, type Ordering } from "../../core/data.ts";
export const ReducerSum = { initial: 0n, combine: (a: bigint, b: bigint): bigint => a + b };
export const ReducerMultiply = { initial: 1n, combine: (a: bigint, b: bigint): bigint => a * b };
export const CombinerMax = { combine: (a: bigint, b: bigint): bigint => a > b ? a : b };
export const CombinerMin = { combine: (a: bigint, b: bigint): bigint => a < b ? a : b };
export const abs = (value: bigint): bigint => value < 0n ? -value : value;
export const sum = (that: bigint) => (self: bigint): bigint => self + that;
export const subtract = (that: bigint) => (self: bigint): bigint => self - that;
export const multiply = (that: bigint) => (self: bigint): bigint => self * that;
export const increment = (value: bigint): bigint => value + 1n;
export const decrement = (value: bigint): bigint => value - 1n;
export const gcd = (that: bigint) => (self: bigint): bigint => {
  let a = abs(self);
  let b = abs(that);
  while (b !== 0n) [a, b] = [b, a % b];
  return a;
};
export const lcm = (that: bigint) => (self: bigint): bigint =>
  self === 0n || that === 0n ? 0n : abs((self / gcd(that)(self)) * that);
export const sqrt = (value: bigint): Option<bigint> => {
  if (value < 0n) return none;
  if (value < 2n) return some(value);
  let guess = value / 2n;
  while (guess * guess > value) guess = (guess + value / guess) / 2n;
  return some(guess);
};
export const sumAll = (values: Iterable<bigint>): bigint => {
  let result = 0n;
  for (const value of values) result += value;
  return result;
};
export const multiplyAll = (values: Iterable<bigint>): bigint => {
  let result = 1n;
  for (const value of values) result *= value;
  return result;
};
export const min = (that: bigint) => (self: bigint): bigint => self < that ? self : that;
export const max = (that: bigint) => (self: bigint): bigint => self > that ? self : that;
export const isLessThan = (that: bigint) => (self: bigint): boolean => self < that;
export const isLessThanOrEqualTo = (that: bigint) => (self: bigint): boolean => self <= that;
export const isGreaterThan = (that: bigint) => (self: bigint): boolean => self > that;
export const isGreaterThanOrEqualTo = (that: bigint) => (self: bigint): boolean => self >= that;
interface Bounds { readonly minimum: bigint; readonly maximum: bigint }
export const between = (bounds: Bounds) => (self: bigint): boolean => self >= bounds.minimum && self <= bounds.maximum;
export const clamp = (bounds: Bounds) => (self: bigint): bigint =>
  self < bounds.minimum ? bounds.minimum : self > bounds.maximum ? bounds.maximum : self;
export const toNumber = (value: bigint): Option<number> =>
  value > BigInt(Number.MAX_SAFE_INTEGER) || value < BigInt(Number.MIN_SAFE_INTEGER) ? none : some(Number(value));
export const fromNumber = (value: number): Option<bigint> => Number.isSafeInteger(value) ? some(BigInt(value)) : none;
export const fromString = (value: string): Option<bigint> => {
  if (value.trim() === "") return none;
  try { return some(BigInt(value)); }
  catch { return none; }
};
export const flickSign = (value: bigint): Ordering => value < 0n ? LT : value > 0n ? GT : EQ;
