// Fixed-width integer and binary32 floating-point runtime helpers.
//
// JS representation:
//   Int8 / Int16 / Int32 (Int) / UInt8 / UInt16 / UInt32 / Float32 -> number
//   Int64 / UInt64 -> bigint (clamped via BigInt.asIntN / BigInt.asUintN)

export const showInt64 = (n: bigint): string => String(n);
export const showUInt64 = (n: bigint): string => String(n);

// Wrapping / widening conversions from any numeric primitive (number or bigint).
export const toInt8 = (x: number | bigint): number =>
  typeof x === "bigint" ? Number(BigInt.asIntN(8, x)) : (x << 24) >> 24;

export const toInt16 = (x: number | bigint): number =>
  typeof x === "bigint" ? Number(BigInt.asIntN(16, x)) : (x << 16) >> 16;

export const toInt32 = (x: number | bigint): number =>
  typeof x === "bigint" ? Number(BigInt.asIntN(32, x)) : x | 0;

export const toInt64 = (x: number | bigint): bigint =>
  typeof x === "bigint" ? BigInt.asIntN(64, x) : BigInt.asIntN(64, BigInt(Math.trunc(x)));

export const toUInt8 = (x: number | bigint): number =>
  typeof x === "bigint" ? Number(BigInt.asUintN(8, x)) : x & 0xff;

export const toUInt16 = (x: number | bigint): number =>
  typeof x === "bigint" ? Number(BigInt.asUintN(16, x)) : x & 0xffff;

export const toUInt32 = (x: number | bigint): number =>
  typeof x === "bigint" ? Number(BigInt.asUintN(32, x)) : x >>> 0;

export const toUInt64 = (x: number | bigint): bigint =>
  typeof x === "bigint" ? BigInt.asUintN(64, x) : BigInt.asUintN(64, BigInt(Math.trunc(x)));

const bigintToFloat32 = (n: bigint): number => {
  if (n === 0n) return 0;
  const neg = n < 0n;
  const abs = neg ? -n : n;
  if (abs < 9007199254740992n) return Math.fround(Number(n));
  const bits = abs.toString(2).length;
  if (bits > 129) return neg ? -Infinity : Infinity;
  const shift = BigInt(bits - 27);
  const mask = (1n << shift) - 1n;
  const sticky = (abs & mask) !== 0n ? 1n : 0n;
  const reduced = ((abs >> shift) | sticky) << shift;
  const f = Math.fround(Number(reduced));
  return neg ? -f : f;
};

export const toFloat32 = (x: number | bigint): number =>
  typeof x === "bigint" ? bigintToFloat32(x) : Math.fround(x);

export const toFloat64 = (x: number | bigint): number => (typeof x === "bigint" ? Number(x) : x);

export const toBigInt = (x: number | bigint): bigint => (typeof x === "bigint" ? x : BigInt(x));

// Fixed-width division, remainder and exponentiation for JS backend codegen.
export const divInt8 = (a: number, b: number): number => (b === 0 ? 0 : ((a / b) << 24) >> 24);
export const remInt8 = (a: number, b: number): number => (b === 0 ? 0 : ((a % b) << 24) >> 24);

export const divInt16 = (a: number, b: number): number => (b === 0 ? 0 : ((a / b) << 16) >> 16);
export const remInt16 = (a: number, b: number): number => (b === 0 ? 0 : ((a % b) << 16) >> 16);

export const divUInt = (a: number, b: number): number => (b === 0 ? 0 : (a / b) >>> 0);
export const remUInt = (a: number, b: number): number => (b === 0 ? 0 : (a % b) >>> 0);

const powSigned32 = (a: number, b: number, shift: number): number => {
  if (b < 0) return a === 1 ? 1 : a === -1 ? ((b & 1) === 0 ? 1 : -1) : 0;
  let result = 1;
  for (let base = a, e = b; e > 0; e >>>= 1) {
    if (e & 1) result = Math.imul(result, base);
    base = Math.imul(base, base);
  }
  return (result << shift) >> shift;
};

const powUnsigned32 = (a: number, b: number): number => {
  let result = 1;
  for (let base = a, e = b >>> 0; e > 0; e >>>= 1) {
    if (e & 1) result = Math.imul(result, base);
    base = Math.imul(base, base);
  }
  return result >>> 0;
};

export const powInt8 = (a: number, b: number): number => powSigned32(a, b, 24);
export const powInt16 = (a: number, b: number): number => powSigned32(a, b, 16);
export const powUInt8 = (a: number, b: number): number => powUnsigned32(a, b) & 0xff;
export const powUInt16 = (a: number, b: number): number => powUnsigned32(a, b) & 0xffff;
export const powUInt32 = (a: number, b: number): number => powUnsigned32(a, b);

export const divInt64 = (a: bigint, b: bigint): bigint => (b === 0n ? 0n : BigInt.asIntN(64, a / b));
export const remInt64 = (a: bigint, b: bigint): bigint => (b === 0n ? 0n : BigInt.asIntN(64, a % b));

export const powInt64 = (a: bigint, b: bigint): bigint => {
  if (b < 0n) return a === 1n ? 1n : a === -1n ? ((b & 1n) === 0n ? 1n : -1n) : 0n;
  let result = 1n;
  for (let base = a, e = b; e > 0n; e >>= 1n) {
    if ((e & 1n) === 1n) result = BigInt.asIntN(64, result * base);
    if (e > 1n) base = BigInt.asIntN(64, base * base);
  }
  return result;
};

export const divUInt64 = (a: bigint, b: bigint): bigint => (b === 0n ? 0n : a / b);
export const remUInt64 = (a: bigint, b: bigint): bigint => (b === 0n ? 0n : a % b);

export const powUInt64 = (a: bigint, b: bigint): bigint => {
  let result = 1n;
  for (let base = a, e = b; e > 0n; e >>= 1n) {
    if ((e & 1n) === 1n) result = BigInt.asUintN(64, result * base);
    if (e > 1n) base = BigInt.asUintN(64, base * base);
  }
  return result;
};

// 64-bit bitwise and shift primitives (shift counts masked mod 64).
export const int64BitAnd = (a: bigint, b: bigint): bigint => a & b;
export const int64BitOr = (a: bigint, b: bigint): bigint => a | b;
export const int64BitXor = (a: bigint, b: bigint): bigint => a ^ b;
export const int64BitNot = (a: bigint): bigint => ~a;
export const int64ShiftLeft = (a: bigint, n: number): bigint => BigInt.asIntN(64, a << BigInt(n & 63));
export const int64ShiftRight = (a: bigint, n: number): bigint => a >> BigInt(n & 63);
export const int64ShiftRightUnsigned = (a: bigint, n: number): bigint =>
  BigInt.asIntN(64, BigInt.asUintN(64, a) >> BigInt(n & 63));
