// Effect's `BigDecimal` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: a scale must
// be a safe integer, and a rounding mode is `BigDecimal.RoundingMode`.
import { BigDecimal, type Result } from "effect";

import { type Ordering, type Tagged, err, ok } from "../data.ts";
import { constructorName, tag } from "@flick-lang/runtime/tags";
import { toOrdering } from "./ordering.ts";

export * from "effect/BigDecimal";

type Decimal = BigDecimal.BigDecimal;

const DECIMAL = /*#__PURE__*/ tag("BigDecimal.DecimalError.DecimalError");

type DecimalError = Tagged<typeof DECIMAL, string>;

const scaled = (scale: number, f: () => Decimal): Result.Result<Decimal, DecimalError> =>
  Number.isSafeInteger(scale) ? ok(f()) : err({ _tag: DECIMAL, _0: `scale ${scale} is not a safe integer` });

/** `BigDecimal.sign`: as Flick's `Ordering`. */
export const flickSign = (n: Decimal): Ordering => toOrdering(BigDecimal.sign(n));

/** `BigDecimal.set_scale scale self`. */
export const flickScale = (scale: number, self: Decimal): Result.Result<Decimal, DecimalError> =>
  scaled(scale, () => BigDecimal.scale(self, scale));

/** `BigDecimal.truncate scale self`. */
export const flickTruncate = (scale: number, self: Decimal): Result.Result<Decimal, DecimalError> =>
  scaled(scale, () => BigDecimal.truncate(self, scale));

/** `BigDecimal.ceil scale self`. */
export const flickCeil = (scale: number, self: Decimal): Result.Result<Decimal, DecimalError> =>
  scaled(scale, () => BigDecimal.ceil(self, scale));

/** `BigDecimal.floor scale self`. */
export const flickFloor = (scale: number, self: Decimal): Result.Result<Decimal, DecimalError> =>
  scaled(scale, () => BigDecimal.floor(self, scale));

const MODES: Record<string, BigDecimal.RoundingMode> = {
  TowardZero: "to-zero",
  AwayFromZero: "from-zero",
  Floor: "floor",
  Ceiling: "ceil",
  HalfAwayFromZero: "half-from-zero",
  HalfTowardZero: "half-to-zero",
  HalfFloor: "half-floor",
  HalfCeiling: "half-ceil",
  HalfEven: "half-even",
  HalfOdd: "half-odd",
};

/** `BigDecimal.round scale mode self`; `Exact` fails when digits would be lost. */
export const flickRound = (
  scale: number,
  mode: Readonly<{ _tag: string }>,
  self: Decimal,
): Result.Result<Decimal, DecimalError> => {
  if (!Number.isSafeInteger(scale)) return err({ _tag: DECIMAL, _0: `scale ${scale} is not a safe integer` });
  const name = constructorName(mode._tag);
  const effectMode = MODES[name];
  if (effectMode !== undefined) return ok(BigDecimal.round(self, { scale, mode: effectMode }));
  const cut = BigDecimal.truncate(self, scale);
  return BigDecimal.equals(cut, self) ? ok(cut) : err({ _tag: DECIMAL, _0: "rounding is needed, and the mode is Exact" });
};
