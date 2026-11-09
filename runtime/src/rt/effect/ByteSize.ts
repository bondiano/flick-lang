// Exact non-negative byte counts. Decimal input is scaled with bigint, so a
// fractional quantity is accepted only when it denotes whole bytes.
import { none, some, type Option } from "../../core/data.ts";
import { item } from "../../internal.ts";
import { constructorName } from "../../tags.ts";

type Size = bigint;
interface Unit { readonly symbol: string; readonly factor: bigint; readonly names: ReadonlyArray<string> }
const decimal = ["B", "kB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB", "RB", "QB"];
const binary = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB", "ZiB", "YiB"];
const decimalNames = ["byte", "kilobyte", "megabyte", "gigabyte", "terabyte", "petabyte", "exabyte", "zettabyte", "yottabyte", "ronnabyte", "quettabyte"];
const binaryNames = ["byte", "kibibyte", "mebibyte", "gibibyte", "tebibyte", "pebibyte", "exbibyte", "zebibyte", "yobibyte"];
const units = (symbols: ReadonlyArray<string>, names: ReadonlyArray<string>, base: bigint): ReadonlyArray<Unit> =>
  symbols.map((symbol, index) => ({ symbol, factor: base ** BigInt(index), names: [symbol, names[index] ?? "", `${names[index]}s`] }));
const decimalUnits = units(decimal, decimalNames, 1000n);
const binaryUnits = units(binary, binaryNames, 1024n);
const allUnits = [...decimalUnits, ...binaryUnits.slice(1)];
const byName = new Map(allUnits.flatMap((unit) => unit.names.map((name) => [name, unit] as const)));
const bySymbol = new Map(allUnits.map((unit) => [unit.symbol, unit] as const));
export const zero: Size = 0n;
const scaleDecimal = (quantity: string, factor: bigint): bigint | undefined => {
  const match = /^(\d+)(?:\.(\d+))?(?:e([+-]?\d+))?$/.exec(quantity);
  if (!match) return undefined;
  const fraction = match[2] ?? "";
  const numerator = BigInt(`${match[1]}${fraction}`) * factor;
  const shift = Number(match[3] ?? 0) - fraction.length;
  if (shift >= 0) return numerator * 10n ** BigInt(shift);
  const divisor = 10n ** BigInt(-shift);
  return numerator % divisor === 0n ? numerator / divisor : undefined;
};
const checked = (number: number, unit: Unit): Option<Size> => {
  if (!Number.isFinite(number) || number < 0) return none;
  const value = scaleDecimal(String(number), unit.factor);
  return value !== undefined && value <= BigInt(Number.MAX_SAFE_INTEGER) ? some(value) : none;
};
const of = (symbol: string) => (number: number): Option<Size> => checked(number, bySymbol.get(symbol) ?? item(decimalUnits, 0));
export const flickBytes = of("B");
export const flickKilobytes = of("kB");
export const flickMegabytes = of("MB");
export const flickGigabytes = of("GB");
export const flickTerabytes = of("TB");
export const flickPetabytes = of("PB");
export const flickExabytes = of("EB");
export const flickZettabytes = of("ZB");
export const flickYottabytes = of("YB");
export const flickRonnabytes = of("RB");
export const flickQuettabytes = of("QB");
export const flickKibibytes = of("KiB");
export const flickMebibytes = of("MiB");
export const flickGibibytes = of("GiB");
export const flickTebibytes = of("TiB");
export const flickPebibytes = of("PiB");
export const flickExbibytes = of("EiB");
export const flickZebibytes = of("ZiB");
export const flickYobibytes = of("YiB");
export const fromString = (text: string): Option<Size> => {
  const match = /^\s*(\d+(?:\.\d+)?)\s*([A-Za-z]+)\s*$/.exec(text);
  const unit = match ? byName.get(match[2] ?? "") : undefined;
  if (!match || !unit) return none;
  const value = scaleDecimal(match[1] ?? "", unit.factor);
  return value === undefined ? none : some(value);
};
export const isZero = (size: Size): boolean => size === 0n;
export const toBigInt = (size: Size): bigint => size;
export const toNumber = (size: Size): Option<number> => size <= BigInt(Number.MAX_SAFE_INTEGER) ? some(Number(size)) : none;
const symbolOf = (unit: { readonly _tag: string }): string => {
  const name = constructorName(unit._tag);
  return name === "KB" ? "kB" : name;
};
export const flickToUnit = (unit: { readonly _tag: string }, size: Size): number =>
  Number(size) / Number(bySymbol.get(symbolOf(unit))?.factor ?? 1n);
export const equals = (that: Size) => (self: Size): boolean => self === that;
interface Bounds { readonly minimum: Size; readonly maximum: Size }
export const between = (bounds: Bounds) => (self: Size): boolean => self >= bounds.minimum && self <= bounds.maximum;
export const clamp = (bounds: Bounds) => (self: Size): Size => self < bounds.minimum ? bounds.minimum : self > bounds.maximum ? bounds.maximum : self;
export const min = (that: Size) => (self: Size): Size => self < that ? self : that;
export const max = (that: Size) => (self: Size): Size => self > that ? self : that;
export const isLessThan = (that: Size) => (self: Size): boolean => self < that;
export const isLessThanOrEqualTo = (that: Size) => (self: Size): boolean => self <= that;
export const isGreaterThan = (that: Size) => (self: Size): boolean => self > that;
export const isGreaterThanOrEqualTo = (that: Size) => (self: Size): boolean => self >= that;
export const sum = (that: Size) => (self: Size): Size => self + that;
export const subtract = (that: Size) => (self: Size): Option<Size> => self < that ? none : some(self - that);
const scalar = (value: number, positive: boolean): bigint | undefined =>
  Number.isSafeInteger(value) && value >= (positive ? 1 : 0) ? BigInt(value) : undefined;
export const times = (multiplier: number) => (self: Size): Option<Size> => {
  const value = scalar(multiplier, false);
  return value === undefined ? none : some(self * value);
};
export const divide = (divisor: number) => (self: Size): Option<Size> => {
  const value = scalar(divisor, true);
  return value === undefined ? none : some(self / value);
};
const formatUnit = (value: Size, unit: Unit, precision: number, trailingZeros: boolean): string => {
  const scale = 10n ** BigInt(precision);
  const rounded = (value * scale * 2n + unit.factor) / (unit.factor * 2n);
  const whole = rounded / scale;
  if (precision === 0) return `${whole} ${unit.symbol}`;
  let fraction = `${rounded % scale}`.padStart(precision, "0");
  if (!trailingZeros) fraction = fraction.replace(/0+$/, "");
  return `${whole}${fraction ? `.${fraction}` : ""} ${unit.symbol}`;
};
interface FormatOptions {
  readonly precision: Option<number>;
  readonly trailingZeros: Option<boolean>;
  readonly binary: boolean;
  readonly unit: Option<{ readonly _tag: string }>;
}
export const flickFormat = (options: FormatOptions, value: Size): string => {
  const raw = options.precision._tag === "Some" ? options.precision.value : 2;
  const precision = Number.isNaN(raw) ? 2 : Math.min(20, Math.max(0, Math.trunc(raw)));
  const trailingZeros = options.trailingZeros._tag === "Some" ? options.trailingZeros.value : false;
  if (options.unit._tag === "Some") return formatUnit(value, bySymbol.get(symbolOf(options.unit.value)) ?? item(decimalUnits, 0), precision, trailingZeros);
  const choices = options.binary ? binaryUnits : decimalUnits;
  if (value === 0n) return "0 B";
  let index = choices.length - 1;
  while (index > 0 && value < item(choices, index).factor) index--;
  if (index < choices.length - 1) {
    const scale = 10n ** BigInt(precision);
    const rounded = (value * scale * 2n + item(choices, index).factor) / (item(choices, index).factor * 2n);
    if (rounded >= (options.binary ? 1024n : 1000n) * scale) index++;
  }
  return formatUnit(value, item(choices, index), precision, trailingZeros);
};
export const ReducerSum = { initial: 0n, combine: (a: Size, b: Size): Size => a + b };
export const CombinerMax = { combine: (a: Size, b: Size): Size => a > b ? a : b };
export const CombinerMin = { combine: (a: Size, b: Size): Size => a < b ? a : b };
