// BigDecimals of Arbitrary, without bounds as a schema of this runtime
// states them: scales and coefficients by families, shrinking by bisection
// toward the closest passing value.
import { item } from "../../internal.ts";
import { type Decimal, decimal, normalize } from "../decimal.ts";
import { type Gen, type Pull, type Sample, compiledGen, lazyShrinks, sampleOf } from "./model.ts";
import { randomBigInt, randomBool, randomIndex, randomInt } from "./random.ts";

const shrinkExponents = [-512, -256, -128, -64, -32, -16, -8, -4, -2, -1, 0, 1, 2, 4, 8, 16, 32, 64, 128, 256, 512];
const MAX_POWER = 4096;

const compare = (a: Decimal, b: Decimal): number => {
  const scale = Math.max(a.scale, b.scale);
  const x = a.value * 10n ** BigInt(scale - a.scale);
  const y = b.value * 10n ** BigInt(scale - b.scale);
  return x < y ? -1 : x > y ? 1 : 0;
};

// all offsets are nonnegative: clamped before a finer scale
const addScale = (scale: number, offset: number, maximum = Number.MAX_SAFE_INTEGER): number => Math.min(maximum, scale + offset);

/** The value at a finer scale, undefined past 4096 digits. */
const projectExact = (d: Decimal, scale: number): bigint | undefined => {
  const difference = scale - d.scale;
  return difference < 0 || difference > MAX_POWER ? undefined : d.value * 10n ** BigInt(difference);
};

const midpoint = (passing: Decimal, failing: Decimal, maximumScale: number): Decimal | undefined => {
  const scale = addScale(Math.max(passing.scale, failing.scale), 1, maximumScale);
  const left = projectExact(passing, scale);
  const right = projectExact(failing, scale);
  if (left === undefined || right === undefined) return undefined;
  const value = left + (right - left) / 2n;
  return value === left || value === right ? undefined : decimal(value, scale);
};

/** The closest passing value and how often it was refined. */
type Context = readonly [passing: Decimal, refinement: number];

const ZERO = /*#__PURE__*/ decimal(0n, 0);

const decimalSample = (value: Decimal, context: Context | undefined, maximumScale: number): Sample =>
  lazyShrinks(value, (): Pull | undefined => {
    const from = context?.[0] ?? ZERO;
    const direction = compare(value, from);
    if (direction === 0) return undefined;
    let next: Decimal | undefined;
    if (context !== undefined) {
      const normalized = normalize(context[0]);
      const scale = addScale(normalized.scale, Math.min(context[1] + 1, 128), maximumScale);
      const projected = projectExact(normalized, scale);
      if (projected !== undefined) next = decimal(projected + BigInt(direction), scale);
    }
    const candidates = (context === undefined ? shrinkExponents.map((e) => decimal(BigInt(direction), -e)) : [next])
      .filter((c): c is Decimal => c !== undefined && c.scale <= maximumScale &&
        compare(c, from) === direction && compare(c, value) === -direction);
    if (context === undefined) candidates.unshift(ZERO);
    let previous = context;
    let index = 0;
    return () => {
      // past the simpler candidates, bisect toward the failing value
      const c = index < candidates.length ? item(candidates, index++)
        : previous === undefined ? undefined : midpoint(previous[0], value, maximumScale);
      if (c === undefined) return undefined;
      const child = decimalSample(c, previous, maximumScale);
      previous = [c, (previous?.[1] ?? 0) + 1];
      return child;
    };
  });

const special = [ZERO, decimal(1n, 1), decimal(1n, 0), decimal(10n, 0), decimal(-1n, 1), decimal(-1n, 0), decimal(-10n, 0)];

export const decimalNode = (): Gen => compiledGen([], () => 0, (st) => {
  let value: Decimal;
  if (randomInt(st, 0, 7) === 0) value = item(special, randomIndex(st, special.length));
  else {
    const scale = randomInt(st, 0, 7) === 6 ? randomInt(st, -6, 20) : randomInt(st, 1, 128) - 1 - randomInt(st, -512, 512);
    const bits = randomInt(st, 0, 3) !== 0 ? randomInt(st, 0, 20) : randomInt(st, 1, 425);
    const lowest = bits === 0 ? 0n : 1n << BigInt(bits - 1);
    const distance = bits === 0 ? 0n : randomBigInt(st, lowest, (lowest << 1n) - 1n);
    const coefficient = randomBool(st) ? distance : -distance;
    if (randomInt(st, 0, 7) === 0 && scale < Number.MAX_SAFE_INTEGER) {
      const trailing = randomInt(st, 1, Math.min(8, Number.MAX_SAFE_INTEGER - scale));
      value = decimal(coefficient * 10n ** BigInt(trailing), scale + trailing);
    } else value = decimal(coefficient, scale);
  }
  return st.shrinks ? decimalSample(value, undefined, addScale(Math.max(value.scale, 0), 128)) : sampleOf(value);
});
