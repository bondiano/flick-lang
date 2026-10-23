// Effect's `ByteSize` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: a unit is a
// `ByteSize.ByteUnit`, and a constructor refuses with `None`, not a throw.
import { ByteSize, Option } from "effect";

import { constructorName } from "../tags.ts";

export * from "effect/ByteSize";

type Size = ByteSize.ByteSize;
type Tagged = Readonly<{ _tag: string }>;

/** A constructor's `None` instead of its throw on a negative or inexact size. */
const checked =
  (f: (value: number) => Size) =>
  (value: number): Option.Option<Size> => {
    try {
      return Option.some(f(value));
    } catch {
      return Option.none();
    }
  };

export const flickBytes = checked(ByteSize.bytes);
export const flickKilobytes = checked(ByteSize.kilobytes);
export const flickMegabytes = checked(ByteSize.megabytes);
export const flickGigabytes = checked(ByteSize.gigabytes);
export const flickTerabytes = checked(ByteSize.terabytes);
export const flickPetabytes = checked(ByteSize.petabytes);
export const flickExabytes = checked(ByteSize.exabytes);
export const flickZettabytes = checked(ByteSize.zettabytes);
export const flickYottabytes = checked(ByteSize.yottabytes);
export const flickRonnabytes = checked(ByteSize.ronnabytes);
export const flickQuettabytes = checked(ByteSize.quettabytes);
export const flickKibibytes = checked(ByteSize.kibibytes);
export const flickMebibytes = checked(ByteSize.mebibytes);
export const flickGibibytes = checked(ByteSize.gibibytes);
export const flickTebibytes = checked(ByteSize.tebibytes);
export const flickPebibytes = checked(ByteSize.pebibytes);
export const flickExbibytes = checked(ByteSize.exbibytes);
export const flickZebibytes = checked(ByteSize.zebibytes);
export const flickYobibytes = checked(ByteSize.yobibytes);

const DECIMAL: ReadonlyArray<ByteSize.DecimalUnit> = ["B", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB", "RB", "QB"];
const BINARY: ReadonlyArray<ByteSize.BinaryUnit> = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];

/** The symbol of a `ByteSize.ByteUnit`: its constructor name, `KB` as `kB`. */
const unit = (u: Tagged): ByteSize.Unit => {
  const name = constructorName(u._tag);
  const symbol = name === "KB" ? "kB" : name;
  return DECIMAL.find((d) => d === symbol) ?? BINARY.find((d) => d === symbol) ?? "B";
};

const isDecimal = (u: ByteSize.Unit): u is ByteSize.DecimalUnit => DECIMAL.some((d) => d === u);

/** `ByteSize.to_unit unit self`. */
export const flickToUnit = (u: Tagged, self: Size): number => ByteSize.toUnit(self, unit(u));

/** `ByteSize.FormatOptions`. */
export interface FormatOptions {
  readonly precision: Option.Option<number>;
  readonly trailingZeros: Option.Option<boolean>;
  readonly binary: boolean;
  readonly unit: Option.Option<Tagged>;
}

/** `ByteSize.format options self`: the precision is cut to an integer from 0 to 20. */
export const flickFormat = (options: FormatOptions, self: Size): string => {
  const p = Option.getOrElse(options.precision, () => 2);
  const precision = Number.isNaN(p) ? 2 : Math.min(20, Math.max(0, Math.trunc(p)));
  const trailingZeros = Option.getOrElse(options.trailingZeros, () => false);
  if (Option.isNone(options.unit)) {
    return ByteSize.format(self, { precision, trailingZeros, system: options.binary ? "binary" : "decimal" });
  }
  const u = unit(options.unit.value);
  return isDecimal(u)
    ? ByteSize.format(self, { precision, trailingZeros, unit: u })
    : ByteSize.format(self, { precision, trailingZeros, unit: u });
};
