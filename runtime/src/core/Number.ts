// `Float` of the standard library without Effect: the bindings of Float.fl; a
// binary function takes `that`, then `self`.
import { EQ, GT, LT, none, type Option, type Ordering, some } from "./data.ts";

/** The order of numbers: NaN is least and equals NaN; -0 equals 0. */
const order = (self: number, that: number): -1 | 0 | 1 => {
  if (self === that) return 0;
  const selfNaN = Number.isNaN(self);
  const thatNaN = Number.isNaN(that);
  if (selfNaN || thatNaN) return selfNaN && thatNaN ? 0 : selfNaN ? -1 : 1;
  return self < that ? -1 : 1;
};

const binary =
  <B>(f: (self: number, that: number) => B) =>
  (that: number) =>
  (self: number): B =>
    f(self, that);

const minOf = (self: number, that: number): number => (self === that || order(self, that) < 1 ? self : that);
const maxOf = (self: number, that: number): number => (self === that || order(self, that) > -1 ? self : that);

export const min = /*#__PURE__*/ binary(minOf);
export const max = /*#__PURE__*/ binary(maxOf);

export const parse = (s: string): Option<number> => {
  if (s === "NaN") return some(NaN);
  if (s === "Infinity") return some(Infinity);
  if (s === "-Infinity") return some(-Infinity);
  if (s.trim() === "") return none;
  const n = Number(s);
  return Number.isNaN(n) ? none : some(n);
};

export const sum = /*#__PURE__*/ binary((self, that) => self + that);
export const subtract = /*#__PURE__*/ binary((self, that) => self - that);
export const multiply = /*#__PURE__*/ binary((self, that) => self * that);
export const divide = /*#__PURE__*/ binary((self, that) => (that === 0 ? none : some(self / that)));

/** `[c, e]` with `n` = c × 10^e exactly as JS prints `n`. */
const toScientificInteger = (n: number): [bigint, number] => {
  if (Number.isInteger(n)) return [BigInt(n), 0];
  const scientific = Math.abs(n).toExponential();
  const eIndex = scientific.indexOf("e");
  const digits = scientific.slice(0, eIndex).replace(".", "");
  const coefficient = BigInt(digits) * (n < 0 ? -1n : 1n);
  return [coefficient, Number(scientific.slice(eIndex + 1)) - digits.length + 1];
};

export const remainder = /*#__PURE__*/ binary((self, divisor) => {
  if (!Number.isFinite(self) || !Number.isFinite(divisor) || divisor === 0) return NaN;
  if (Number.isInteger(self) && Number.isInteger(divisor)) return self % divisor;
  const [selfCoefficient, selfExponent] = toScientificInteger(self);
  const [divisorCoefficient, divisorExponent] = toScientificInteger(divisor);
  const exponent = Math.min(selfExponent, divisorExponent);
  const selfInteger = selfCoefficient * 10n ** BigInt(selfExponent - exponent);
  const divisorInteger = divisorCoefficient * 10n ** BigInt(divisorExponent - exponent);
  const out = selfInteger % divisorInteger;
  if (out === 0n) return self < 0 || Object.is(self, -0) ? -0 : 0;
  const r = Number(`${out}e${exponent}`);
  return r === 0 ? Math.sign(self) * Number.MIN_VALUE : r;
});

export const increment = (n: number): number => n + 1;
export const decrement = (n: number): number => n - 1;

export const sumAll = (xs: ReadonlyArray<number>): number => {
  let out = 0;
  for (const n of xs) out += n;
  return out;
};

export const multiplyAll = (xs: ReadonlyArray<number>): number => {
  let out = 1;
  for (const n of xs) {
    if (n === 0) return 0;
    out *= n;
  }
  return out;
};

export const isLessThan = /*#__PURE__*/ binary((self, that) => order(self, that) === -1);
export const isLessThanOrEqualTo = /*#__PURE__*/ binary((self, that) => order(self, that) !== 1);
export const isGreaterThan = /*#__PURE__*/ binary((self, that) => order(self, that) === 1);
export const isGreaterThanOrEqualTo = /*#__PURE__*/ binary((self, that) => order(self, that) !== -1);

interface Bounds {
  readonly minimum: number;
  readonly maximum: number;
}

export const between =
  (bounds: Bounds) =>
  (self: number): boolean =>
    order(self, bounds.minimum) !== -1 && order(self, bounds.maximum) !== 1;

export const clamp =
  (bounds: Bounds) =>
  (self: number): number =>
    minOf(bounds.maximum, maxOf(bounds.minimum, self));

/** `Number.sign` as Flick's `Ordering`: how `n` compares with 0 by `order`, NaN `LT`. */
export const flickSign = (n: number): Ordering => (n > 0 ? GT : n === 0 ? EQ : LT);

export const nextPow2 = (n: number): number => Math.max(2 ** Math.ceil(Math.log(n) / Math.log(2)), 2);

export const round = /*#__PURE__*/ binary((self, precision) => {
  const factor = 10 ** precision;
  return Math.round(self * factor) / factor;
});
