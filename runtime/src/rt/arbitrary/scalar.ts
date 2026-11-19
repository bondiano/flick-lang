// Booleans, numbers, strings and BigInts of Arbitrary, edge cases first,
// shrinking toward false, zero or the empty string. The schemas of this
// runtime state no bounds, so the only constraint is a number's kind.
import { item } from "../../internal.ts";
import { type Gen, type Pull, type Sample, compiledGen, lazyShrinks, pullEach, sampleOf, shrinkSample } from "./model.ts";
import {
  type GenState, biased, numberIndex, indexNumber, numericInt, randomBigInt, randomBool, randomIndex, randomInt,
  randomLength, randomNumber,
} from "./random.ts";

const free = (): number => 0;

export const booleanNode = (): Gen => compiledGen([], free, (st) => {
  const value = randomBool(st);
  return sampleOf(value, st.shrinks && value ? pullEach([false], (b) => sampleOf(b)) : undefined);
});

/** A candidate and the closest value known to pass. */
interface NumShrink { readonly value: number; readonly context: number | undefined }

const shrinkInteger = (current: number, target: number, asap: boolean): Array<NumShrink> => {
  const out: Array<NumShrink> = [];
  const gap = current - target;
  let previous = asap ? undefined : target;
  for (let toRemove = asap ? gap : Math.trunc(gap / 2); toRemove !== 0; toRemove = Math.trunc(toRemove / 2)) {
    const value = toRemove === gap ? target : current - toRemove;
    out.push({ value, context: previous });
    previous = value;
  }
  return out;
};

// halves the distance of the doubles' indexes
const shrinkNumber = (current: number, target: number, asap: boolean): Array<NumShrink> => {
  if (Number.isNaN(current)) return [{ value: target, context: undefined }];
  const at = numberIndex(current);
  const gap = at - numberIndex(target);
  let previous = asap ? undefined : target;
  const out: Array<NumShrink> = [];
  for (let toRemove = asap ? gap : gap / 2n; toRemove !== 0n; toRemove /= 2n) {
    const value = toRemove === gap ? target : indexNumber(at - toRemove);
    out.push({ value, context: previous });
    previous = value;
  }
  return out;
};

/** A number and its shrinks toward zero. */
const numberSample = (value: number, integer: boolean, context: number | undefined): Sample => {
  let candidates: Array<NumShrink>;
  if (!integer) {
    candidates = context === undefined ? shrinkNumber(value, 0, true)
      : !Number.isNaN(value) && (numberIndex(value) === numberIndex(context) + 1n || numberIndex(value) === numberIndex(context) - 1n)
        ? [{ value: context, context: undefined }]
        : shrinkNumber(value, context, false);
  } else {
    candidates = context === undefined ? shrinkInteger(value, 0, true)
      : (value > 0 && value === context + 1) || (value < 0 && value === context - 1)
        ? [{ value: context, context: undefined }]
        : shrinkInteger(value, context, false);
  }
  return sampleOf(value, candidates.length === 0 ? undefined
    : pullEach(candidates, (c) => numberSample(c.value, integer, c.context)));
};

/** Which numbers a check allows: any, finite ones or integers. */
export type NumberKind = "any" | "finite" | "integer";

export const numberNode = (kind: NumberKind): Gen => {
  const integer = kind === "integer";
  const random = integer ? undefined : randomNumber(kind === "any" ? -Infinity : -Number.MAX_VALUE,
    kind === "any" ? Infinity : Number.MAX_VALUE, kind === "any");
  let magnitude = Number.NaN;
  let randomInteger: ((st: GenState) => number) | undefined;
  return compiledGen([], free, (st) => {
    let value: number;
    if (random === undefined) {
      const next = Math.max(1, st.size * st.size);
      if (randomInteger === undefined || next !== magnitude) {
        magnitude = next;
        randomInteger = numericInt(-next, next);
      }
      value = randomInteger(st);
    } else value = random(st);
    return st.shrinks ? numberSample(value, integer, undefined) : sampleOf(value);
  });
};

const stringEdgeCases = ["", " ", "\t", "\n", "\0", "0", "-1", "4294967295", "__proto__", "constructor", "prototype",
  "toString", "\uD800", "\uDC00", "\u{1F600}"];

const randomString = (st: GenState, maximum: number): string => {
  if (biased(st)) {
    const eligible = stringEdgeCases.filter((s) => s.length <= maximum);
    if (eligible.length > 0) return item(eligible, randomIndex(st, eligible.length));
  }
  const length = randomLength(st, 0, maximum);
  let value = "";
  for (let i = 0; i < length; i++) value += String.fromCharCode(randomInt(st, 32, 126));
  return value;
};

/** Shortens, then lowers each code unit toward NUL. */
const shrinkString = (value: string): ReadonlyArray<string> => {
  const candidates = value.length === 0 ? [] : ["", value.slice(0, Math.floor(value.length / 2)), value.slice(0, -1)];
  for (let i = 0; i < value.length; i++) {
    for (const c of shrinkInteger(value.charCodeAt(i), 0, true))
      candidates.push(value.slice(0, i) + String.fromCharCode(c.value) + value.slice(i + 1));
  }
  return [...new Set(candidates)].filter((c) => c !== value);
};

export const stringNode = (): Gen => compiledGen([], free, (st) => {
  const value = randomString(st, Math.max(0, st.size));
  return st.shrinks ? shrinkSample(value, shrinkString) : sampleOf(value);
});

const semanticMagnitudes = [1n, 2n, 0x7fffffffn, 0x80000000n, BigInt(Number.MAX_SAFE_INTEGER),
  BigInt(Number.MAX_SAFE_INTEGER) + 1n, BigInt(Number.MAX_VALUE), BigInt(Number.MAX_VALUE) + 1n, 1n << 1024n];
const special = [0n, ...semanticMagnitudes.flatMap((m) => [m, -m])];

const bitLength = (value: bigint): number => (value === 0n ? 0 : (value < 0n ? -value : value).toString(2).length);

const randomMagnitude = (st: GenState): bigint => {
  const family = randomInt(st, 0, 7);
  if (family === 0) return 0n;
  const bits = randomInt(st, 1, family <= 3 ? 20 : 2048);
  const lowest = 1n << BigInt(bits - 1);
  return randomBigInt(st, lowest, (lowest << 1n) - 1n);
};

/** Shrinks by bisection toward the closest passing value. */
const bigintSample = (value: bigint, context: bigint | undefined): Sample => lazyShrinks(value, (): Pull | undefined => {
  const from = context ?? 0n;
  const gap = value - from;
  if (gap === 0n) return undefined;
  if (context !== undefined && (gap === 1n || gap === -1n)) return pullEach([context], (c) => bigintSample(c, undefined));
  const direction = gap > 0n ? 1n : -1n;
  const candidates = new Set<bigint>(context === undefined ? [0n] : []);
  const add = (c: bigint): void => {
    if ((c - from) * direction > 0n && (value - c) * direction > 0n) candidates.add(c);
  };
  if (context === undefined) {
    for (const m of semanticMagnitudes) add(direction * m);
    const maximumBits = bitLength(gap);
    for (let bits = 1; bits < maximumBits; bits *= 2) add(direction * ((1n << BigInt(bits)) - 1n));
  } else {
    const passing = bitLength(context);
    const failing = bitLength(value);
    if (Math.abs(failing - passing) > 1 && (context === 0n || (context > 0n) === (value > 0n)))
      add(direction * ((1n << BigInt(Math.floor((passing + failing) / 2))) - 1n));
  }
  let toRemove = gap / 2n;
  for (let step = 0; toRemove !== 0n && step < 256; step++, toRemove /= 2n) add(value - toRemove);
  candidates.add(value - direction);
  const ordered = [...candidates].sort((a, b) => (a < b ? -Number(direction) : a > b ? Number(direction) : 0));
  let previous = context;
  return pullEach(ordered, (c) => {
    const child = bigintSample(c, previous);
    previous = c;
    return child;
  });
});

export const bigintNode = (): Gen => compiledGen([], free, (st) => {
  let value: bigint;
  if (randomInt(st, 0, 7) === 0) value = item(special, randomIndex(st, special.length));
  else {
    const sign = randomBool(st) ? 1n : -1n;
    value = sign * randomMagnitude(st);
  }
  return st.shrinks ? bigintSample(value, undefined) : sampleOf(value);
});
