// Effect's `ByteSize` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: a unit is a
// `ByteSize.ByteUnit`, a constructor refuses with `None`, not a throw, and
// the JSON codecs are fallible ones of the strict exporter.
import { ByteSize, Option, Schema, SchemaTransformation } from "effect";

import { type AnySchema, jsonNumber } from "../schema/schemas.ts";
import { constructorName } from "../tags.ts";
import { fallible } from "./Schema.ts";

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

export const flickBytes = /*#__PURE__*/ checked(ByteSize.bytes);
export const flickKilobytes = /*#__PURE__*/ checked(ByteSize.kilobytes);
export const flickMegabytes = /*#__PURE__*/ checked(ByteSize.megabytes);
export const flickGigabytes = /*#__PURE__*/ checked(ByteSize.gigabytes);
export const flickTerabytes = /*#__PURE__*/ checked(ByteSize.terabytes);
export const flickPetabytes = /*#__PURE__*/ checked(ByteSize.petabytes);
export const flickExabytes = /*#__PURE__*/ checked(ByteSize.exabytes);
export const flickZettabytes = /*#__PURE__*/ checked(ByteSize.zettabytes);
export const flickYottabytes = /*#__PURE__*/ checked(ByteSize.yottabytes);
export const flickRonnabytes = /*#__PURE__*/ checked(ByteSize.ronnabytes);
export const flickQuettabytes = /*#__PURE__*/ checked(ByteSize.quettabytes);
export const flickKibibytes = /*#__PURE__*/ checked(ByteSize.kibibytes);
export const flickMebibytes = /*#__PURE__*/ checked(ByteSize.mebibytes);
export const flickGibibytes = /*#__PURE__*/ checked(ByteSize.gibibytes);
export const flickTebibytes = /*#__PURE__*/ checked(ByteSize.tebibytes);
export const flickPebibytes = /*#__PURE__*/ checked(ByteSize.pebibytes);
export const flickExbibytes = /*#__PURE__*/ checked(ByteSize.exbibytes);
export const flickZebibytes = /*#__PURE__*/ checked(ByteSize.zebibytes);
export const flickYobibytes = /*#__PURE__*/ checked(ByteSize.yobibytes);

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

/** `ByteSize.json`: a size as its text, `"1.5 KiB"`; encodes as `"1536 bytes"`. */
export const flickJson: AnySchema = /*#__PURE__*/ fallible(Schema.ByteSizeFromString);

/** `ByteSize.json_number`: a whole, non-negative JSON number of bytes. */
export const flickJsonNumber: AnySchema = /*#__PURE__*/ fallible(
  jsonNumber.pipe(Schema.decodeTo(Schema.ByteSize, SchemaTransformation.byteSizeFromNumber)),
);

/** `ByteSize.json_bigint`: a number of bytes as a JSON string of digits. */
export const flickJsonBigint: AnySchema = /*#__PURE__*/ fallible(
  Schema.String.check(Schema.isPattern(/^[0-9]+$/)).pipe(
    Schema.decodeTo(Schema.BigInt, SchemaTransformation.bigintFromString),
    Schema.decodeTo(Schema.ByteSize, SchemaTransformation.byteSizeFromBigInt),
  ),
);
