// The randomness of Arbitrary, draw for draw as the Effect adapter and the Go
// runtime: a seed hashed to two words, a xoshiro128** generator per attempt,
// and the uniform and edge-biased draws over it.
import { item } from "../../internal.ts";

/** A hashed seed. */
export interface Seed { readonly first: number; readonly second: number }

// MurmurHash3's fmix32
const mix32 = (value: number): number => {
  value ^= value >>> 16;
  value = Math.imul(value, 0x85ebca6b);
  value ^= value >>> 13;
  value = Math.imul(value, 0xc2b2ae35);
  return (value ^ (value >>> 16)) >>> 0;
};

/** The hash of `typeof seed + ":" + seed`. */
export const hashSeed = (seed: string | number): Seed => {
  const text = `${typeof seed}:${seed}`;
  let first = 0x811c9dc5;
  let second = 0x9e3779b9;
  for (let i = 0; i < text.length; i++) {
    const code = text.charCodeAt(i);
    first = Math.imul(first ^ code, 0x01000193);
    second = Math.imul(second ^ code ^ i, 0x85ebca6b);
  }
  return { first: mix32(first), second: mix32(second ^ text.length) };
};

const rotl = (value: number, shift: number): number => ((value << shift) | (value >>> (32 - shift))) >>> 0;

/** xoshiro128** 1.1, the generator of an attempt. */
export class Random {
  private s0: number;
  private s1: number;
  private s2: number;
  private s3: number;
  constructor(s0: number, s1: number, s2: number, s3: number) {
    this.s0 = s0;
    this.s1 = s1;
    this.s2 = s2;
    this.s3 = s3;
  }
  nextUint32(): number {
    const result = Math.imul(rotl(Math.imul(this.s1, 5), 7), 9) >>> 0;
    const t = this.s1 << 9;
    this.s2 ^= this.s0;
    this.s3 ^= this.s1;
    this.s1 ^= this.s2;
    this.s0 ^= this.s3;
    this.s2 ^= t;
    this.s3 = rotl(this.s3, 11);
    return result;
  }
  /** A double in [0, 1) of 53 random bits. */
  nextDouble(): number {
    const high = this.nextUint32() >>> 5;
    const low = this.nextUint32() >>> 6;
    return (high * 0x4000000 + low) / 0x20000000000000;
  }
  clone(): Random { return new Random(this.s0, this.s1, this.s2, this.s3); }
  copyFrom(other: Random): void {
    this.s0 = other.s0;
    this.s1 = other.s1;
    this.s2 = other.s2;
    this.s3 = other.s3;
  }
}

/** The generator of an attempt, derived from the seed directly so a replay jumps to it. */
export const attemptRandom = (seed: Seed, attempt: number): Random => {
  const low = attempt >>> 0;
  const high = Math.floor(attempt / 0x100000000) >>> 0;
  let s0 = mix32(seed.first ^ low ^ Math.imul(high, 0x9e3779b9));
  const s1 = mix32(seed.second ^ high ^ Math.imul(low, 0x85ebca6b));
  const s2 = mix32(seed.first ^ high ^ Math.imul(low, 0xc2b2ae35) ^ 0x243f6a88);
  const s3 = mix32(seed.second ^ low ^ Math.imul(high, 0x27d4eb2f) ^ 0xb7e15162);
  if ((s0 | s1 | s2 | s3) === 0) s0 = 0x9e3779b9;
  return new Random(s0, s1, s2, s3);
};

/** The state of a generation; a copy shares the generator and the budget unless it replaces them. */
export interface GenState {
  readonly size: number;
  readonly shrinks: boolean;
  readonly biasFactor: number;
  readonly random: Random;
  readonly budget: { remaining: number };
}

export const withSize = (st: GenState, size: number): GenState => ({ ...st, size });
export const withRandom = (st: GenState, random: Random, remaining: number): GenState =>
  ({ ...st, random, budget: { remaining } });

const UINT32_VALUES = 0x100000000;
const DOUBLE_VALUES = 0x20000000000000;

/** A uniform integer in [0, n), n at most 2^32. */
export const below = (st: GenState, n: number): number => {
  const bucket = Math.floor(UINT32_VALUES / n);
  const accepted = bucket * n;
  let value = st.random.nextUint32();
  while (value >= accepted) value = st.random.nextUint32();
  return Math.floor(value / bucket);
};

/** An index into `n` choices; no draw for one. */
export const randomIndex = (st: GenState, n: number): number => (n === 1 ? 0 : below(st, n));

/** A uniform integer in [lo, hi]. */
export const randomInt = (st: GenState, lo: number, hi: number): number => {
  lo = Math.ceil(lo);
  hi = Math.floor(hi);
  if (lo === hi) return lo;
  const width = hi - lo + 1;
  if (width <= UINT32_VALUES) return lo + below(st, width);
  if (width <= DOUBLE_VALUES) {
    const bucket = Math.floor(DOUBLE_VALUES / width);
    const accepted = bucket * width;
    for (;;) {
      const value = Math.floor(st.random.nextDouble() * DOUBLE_VALUES);
      if (value < accepted) return lo + Math.floor(value / bucket);
    }
  }
  return Number(randomBigInt(st, BigInt(lo), BigInt(hi)));
};

export const randomBool = (st: GenState): boolean => (st.random.nextUint32() & 1) === 1;

/** Whether this draw goes to the edges: one in biasFactor. */
export const biased = (st: GenState): boolean => randomInt(st, 1, st.biasFactor) === 1;

/** A length in [lo, hi], sometimes a bound. */
export const randomLength = (st: GenState, lo: number, hi: number): number => {
  if (lo === hi) return lo;
  if (biased(st)) return randomBool(st) ? lo : hi;
  return randomInt(st, lo, hi);
};

/** A uniform integer in [lo, hi]: rejection over its bits, the leading word first. */
export const randomBigInt = (st: GenState, lo: bigint, hi: bigint): bigint => {
  if (lo === hi) return lo;
  const width = hi - lo + 1n;
  const bits = (width - 1n).toString(2).length;
  const leading = ((bits - 1) % 32) + 1;
  for (;;) {
    let value = BigInt(below(st, 2 ** leading));
    for (let rest = bits - leading; rest > 0; rest -= 32) value = (value << 32n) | BigInt(st.random.nextUint32());
    if (value < width) return lo + value;
  }
};

/** The first range two times in three. */
const selectBiased = <A>(st: GenState, values: ReadonlyArray<A>): A => {
  if (values.length === 1) return item(values, 0);
  const i = randomInt(st, -2 * (values.length - 1), values.length - 2);
  return item(values, i < 0 ? 0 : i + 1);
};

/** Uniform in [lo, hi], or near an edge. */
export const numericInt = (lo: number, hi: number): ((st: GenState) => number) => {
  let ranges: ReadonlyArray<readonly [number, number]>;
  if (lo === hi) ranges = [[lo, hi]];
  else if (lo < 0 && hi > 0) {
    const low = Math.floor(Math.log2(-lo));
    const high = Math.floor(Math.log2(hi));
    ranges = [[-low, high], [hi - high, hi], [lo, lo + low]];
  } else {
    const gap = Math.floor(Math.log2(hi - lo));
    const near: ReadonlyArray<readonly [number, number]> = [[lo, lo + gap], [hi - gap, hi]];
    ranges = lo < 0 ? [item(near, 1), item(near, 0)] : near;
  }
  return (st) => {
    if (!biased(st)) return randomInt(st, lo, hi);
    const [from, to] = selectBiased(st, ranges);
    return randomInt(st, from, to);
  };
};

// the number of decimal digits, 0 for 0
const logLike = (value: bigint): bigint => (value === 0n ? 0n : BigInt(value.toString().length));

/** Uniform in [lo, hi], or near an edge. */
export const numericBigInt = (lo: bigint, hi: bigint): ((st: GenState) => bigint) => {
  let ranges: ReadonlyArray<readonly [bigint, bigint]>;
  if (lo === hi) ranges = [[lo, hi]];
  else if (lo < 0n && hi > 0n) {
    const low = logLike(-lo);
    const high = logLike(hi);
    ranges = [[-low, high], [hi - high, hi], [lo, lo + low]];
  } else {
    const gap = logLike(hi - lo);
    const near: ReadonlyArray<readonly [bigint, bigint]> = [[lo, lo + gap], [hi - gap, hi]];
    ranges = lo < 0n ? [item(near, 1), item(near, 0)] : near;
  }
  return (st) => {
    if (!biased(st)) return randomBigInt(st, lo, hi);
    const [from, to] = selectBiased(st, ranges);
    return randomBigInt(st, from, to);
  };
};

const view = /*#__PURE__*/ new DataView(new ArrayBuffer(8));
const SIGN = 1n << 63n;
const BITS = (1n << 64n) - 1n;

/** The monotone index of a double: -0 is -1, +0 is 0. */
export const numberIndex = (value: number): bigint => {
  view.setFloat64(0, value);
  const bits = view.getBigUint64(0);
  const unsigned = (bits & SIGN) === 0n ? bits | SIGN : ~bits & BITS;
  return unsigned - SIGN;
};

export const indexNumber = (index: bigint): number => {
  const unsigned = index + SIGN;
  view.setBigUint64(0, (unsigned & SIGN) === 0n ? ~unsigned & BITS : unsigned ^ SIGN);
  return view.getFloat64(0);
};

export const nextNumber = (value: number): number =>
  Number.isNaN(value) || value === Infinity ? value : value === 0 ? Number.MIN_VALUE : indexNumber(numberIndex(value) + 1n);

export const previousNumber = (value: number): number =>
  Number.isNaN(value) || value === -Infinity ? value : value === 0 ? -Number.MIN_VALUE : indexNumber(numberIndex(value) - 1n);

/** An index between the bounds' with edge bias, and NaN one past the end when allowed. */
export const randomNumber = (lo: number, hi: number, allowNaN: boolean): ((st: GenState) => number) => {
  const from = numberIndex(lo);
  const to = numberIndex(hi);
  const nanBelow = allowNaN && to <= 0n;
  const index = numericBigInt(nanBelow ? from - 1n : from, allowNaN && !nanBelow ? to + 1n : to);
  return (st) => {
    const i = index(st);
    return i < from || i > to ? Number.NaN : indexNumber(i);
  };
};

/** Fisher–Yates over a copy. */
export const shuffle = <A>(st: GenState, xs: ReadonlyArray<A>): Array<A> => {
  const out = [...xs];
  for (let i = out.length - 1; i >= 1; i--) {
    const j = below(st, i + 1);
    const value = item(out, i);
    out[i] = item(out, j);
    out[j] = value;
  }
  return out;
};
