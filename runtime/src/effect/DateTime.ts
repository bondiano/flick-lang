// Effect's `DateTime` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes: options
// records with `Option` fields, unit constructors instead of strings, and
// `DateTime.DateTimeError` instead of `None` or a throw; the JSON codecs are
// fallible ones of the strict exporter.
import { DateTime, Duration, Option, Result, Schema, SchemaTransformation } from "effect";

import { EQ, GT, LT, type Ordering, type Tagged, err, none, ok, some } from "../data.ts";
import { type AnySchema, jsonNumber } from "../schema/schemas.ts";
import { constructorName, tag } from "../tags.ts";
import { fallible } from "./Schema.ts";

export * from "effect/DateTime";

const ERROR = /*#__PURE__*/ tag("DateTime.DateTimeError.DateTimeError");

/** `DateTime.DateTimeError`. */
export type DateTimeError = Tagged<typeof ERROR, string>;

type Checked<A> = Result.Result<A, DateTimeError>;
type Constructor = Readonly<{ _tag: string }>;
type Parts = DateTime.DateTime.PartsWithWeekday;
type WeekStart = 0 | 1 | 2 | 3 | 4 | 5 | 6;

const fail = (message: string): Checked<never> => err({ _tag: ERROR, _0: message });

/** The value, or the error `what` when Effect gave none or an invalid date. */
const checked = <A extends DateTime.DateTime>(what: string, value: Option.Option<A>): Checked<A> =>
  Option.isSome(value) && Number.isFinite(value.value.epochMilliseconds) ? ok(value.value) : fail(what);

/** `f()`, with a throw (an invalid zone, a rejected wall-clock time, a bad locale) as the error. */
const attempt = <A>(what: string, f: () => A): Checked<A> => {
  try {
    return ok(f());
  } catch (e) {
    return fail(`${what}: ${e instanceof Error ? e.message : String(e)}`);
  }
};

/** What a constructor without fields stands for in Effect. */
const pick = <A>(table: Readonly<Record<string, A>>, c: Constructor): A => {
  const value = table[constructorName(c._tag)];
  if (value === undefined) throw new Error(`unexpected constructor ${c._tag}`);
  return value;
};

const UNITS: Readonly<Record<string, DateTime.DateTime.UnitSingular>> = {
  Millisecond: "millisecond",
  Second: "second",
  Minute: "minute",
  Hour: "hour",
  Day: "day",
  Week: "week",
  Month: "month",
  Year: "year",
};

const DISAMBIGUATIONS: Readonly<Record<string, DateTime.Disambiguation>> = {
  Compatible: "compatible",
  Earlier: "earlier",
  Later: "later",
  Reject: "reject",
};

const unit = (c: Constructor): DateTime.DateTime.UnitSingular => pick(UNITS, c);

interface ZoneOptions {
  readonly adjustForTimeZone: Option.Option<boolean>;
  readonly disambiguation: Option.Option<Constructor>;
}

const zoneOptions = (o: ZoneOptions): { adjustForTimeZone: boolean | undefined; disambiguation: DateTime.Disambiguation | undefined } => ({
  adjustForTimeZone: Option.getOrUndefined(o.adjustForTimeZone),
  disambiguation: Option.getOrUndefined(Option.map(o.disambiguation, (d) => pick(DISAMBIGUATIONS, d))),
});

type PartialParts = { readonly [K in keyof Parts]: Option.Option<number> };

const partial = (p: PartialParts): Partial<Parts> =>
  Object.fromEntries(Object.entries(p).flatMap(([k, v]: [string, Option.Option<number>]) => (Option.isSome(v) ? [[k, v.value]] : [])));

interface FormatOptions {
  readonly locale: Option.Option<string>;
  readonly dateStyle: Option.Option<string>;
  readonly timeStyle: Option.Option<string>;
}

type Style = NonNullable<Intl.DateTimeFormatOptions["dateStyle"]>;

const isStyle = (s: string): s is Style => s === "full" || s === "long" || s === "medium" || s === "short";

/** A style, or a throw that `attempt` turns into the error. */
const style = (s: string): Style => {
  if (!isStyle(s)) throw new RangeError(`invalid style ${JSON.stringify(s)}`);
  return s;
};

const intl = (o: FormatOptions): Intl.DateTimeFormatOptions & { readonly locale?: string } => ({
  ...Option.match(o.locale, { onNone: () => ({}), onSome: (locale) => ({ locale }) }),
  ...Option.match(o.dateStyle, { onNone: () => ({}), onSome: (s) => ({ dateStyle: style(s) }) }),
  ...Option.match(o.timeStyle, { onNone: () => ({}), onSome: (s) => ({ timeStyle: style(s) }) }),
});

export const flickMake = (s: string): Checked<DateTime.DateTime> => checked(`not a date-time: ${JSON.stringify(s)}`, DateTime.make(s));
export const flickFromParts = (p: PartialParts): Checked<DateTime.DateTime> => checked("invalid date parts", DateTime.make(partial(p)));
export const flickFromEpochSeconds = (n: number): Checked<DateTime.DateTime> =>
  checked(`epoch seconds out of range: ${n}`, DateTime.make(n * 1000));
export const flickMakeZonedFromString = (s: string): Checked<DateTime.DateTime> =>
  checked(`not a zoned date-time: ${JSON.stringify(s)}`, DateTime.makeZonedFromString(s));

export const flickMakeZoned = (o: ZoneOptions, zone: DateTime.TimeZone, self: DateTime.DateTime): Checked<DateTime.DateTime> =>
  checked("cannot place the date-time in the zone", DateTime.makeZoned(self, { ...zoneOptions(o), timeZone: zone }));
export const flickSetZone = (o: ZoneOptions, zone: DateTime.TimeZone, self: DateTime.DateTime): Checked<DateTime.DateTime> =>
  attempt("set_zone", () => DateTime.setZone(self, zone, zoneOptions(o)));
export const flickSetZoneOffset = (o: ZoneOptions, offset: number, self: DateTime.DateTime): Checked<DateTime.DateTime> =>
  attempt("set_zone_offset", () => DateTime.setZoneOffset(self, offset, zoneOptions(o)));
export const flickSetZoneNamed = (o: ZoneOptions, id: string, self: DateTime.DateTime): Checked<DateTime.DateTime> => {
  const zoned = attempt("set_zone_named", () => DateTime.setZoneNamed(self, id, zoneOptions(o)));
  return Result.flatMap(zoned, (z) => Option.match(z, { onNone: () => fail(`invalid time zone: ${JSON.stringify(id)}`), onSome: ok }));
};

export const flickZoneMakeNamed = (id: string): Checked<DateTime.TimeZone> =>
  Option.match(DateTime.zoneMakeNamed(id), { onNone: () => fail(`invalid time zone: ${JSON.stringify(id)}`), onSome: ok });
export const flickZoneFromString = (s: string): Checked<DateTime.TimeZone> =>
  Option.match(DateTime.zoneFromString(s), { onNone: () => fail(`invalid time zone: ${JSON.stringify(s)}`), onSome: ok });

export const flickNow = (): typeof DateTime.now => DateTime.now;

/** The time between the two, whichever comes first: Flick durations are never negative. */
export const flickDistance = (other: DateTime.DateTime, self: DateTime.DateTime): Duration.Duration =>
  Duration.abs(DateTime.distance(self, other));

export const flickCompare = (a: DateTime.DateTime, b: DateTime.DateTime): Ordering => {
  const n = DateTime.Order(a, b);
  return n < 0 ? LT : n > 0 ? GT : EQ;
};

export const flickZonedOffset = (self: DateTime.DateTime): Option.Option<number> =>
  DateTime.isZoned(self) ? some(DateTime.zonedOffset(self)) : none;
export const flickZonedOffsetIso = (self: DateTime.DateTime): Option.Option<string> =>
  DateTime.isZoned(self) ? some(DateTime.zonedOffsetIso(self)) : none;
export const flickFormatIsoZoned = (self: DateTime.DateTime): Option.Option<string> =>
  DateTime.isZoned(self) ? some(DateTime.formatIsoZoned(self)) : none;

export const flickSetParts = (p: PartialParts, self: DateTime.DateTime): DateTime.DateTime => DateTime.setParts(self, partial(p));
export const flickSetPartsUtc = (p: PartialParts, self: DateTime.DateTime): DateTime.DateTime => DateTime.setPartsUtc(self, partial(p));

const math = (c: Constructor, n: number): Partial<DateTime.DateTime.PartsForMath> => ({ [`${unit(c)}s`]: n });

export const flickAdd = (c: Constructor, n: number, self: DateTime.DateTime): DateTime.DateTime => DateTime.add(self, math(c, n));
export const flickSubtract = (c: Constructor, n: number, self: DateTime.DateTime): DateTime.DateTime =>
  DateTime.subtract(self, math(c, n));

const isWeekStart = (n: number): n is WeekStart => Number.isInteger(n) && n >= 0 && n <= 6;

/** The first day of the week, taken modulo 7. */
const week = (o: { readonly weekStartsOn: Option.Option<number> }): { weekStartsOn: WeekStart | undefined } => {
  const n = Option.getOrUndefined(o.weekStartsOn);
  const day = n === undefined ? undefined : ((Math.trunc(n) % 7) + 7) % 7;
  return { weekStartsOn: day !== undefined && isWeekStart(day) ? day : undefined };
};

type WeekOptions = { readonly weekStartsOn: Option.Option<number> };

export const flickStartOf = (o: WeekOptions, c: Constructor, self: DateTime.DateTime): DateTime.DateTime => DateTime.startOf(self, unit(c), week(o));
export const flickEndOf = (o: WeekOptions, c: Constructor, self: DateTime.DateTime): DateTime.DateTime => DateTime.endOf(self, unit(c), week(o));
export const flickNearest = (o: WeekOptions, c: Constructor, self: DateTime.DateTime): DateTime.DateTime => DateTime.nearest(self, unit(c), week(o));

export const flickFormat = (o: FormatOptions, self: DateTime.DateTime): Checked<string> => attempt("format", () => DateTime.format(self, intl(o)));
export const flickFormatLocal = (o: FormatOptions, self: DateTime.DateTime): Checked<string> =>
  attempt("format_local", () => DateTime.formatLocal(self, intl(o)));
export const flickFormatUtc = (o: FormatOptions, self: DateTime.DateTime): Checked<string> => attempt("format_utc", () => DateTime.formatUtc(self, intl(o)));
export const flickFormatIntl = (o: FormatOptions, self: DateTime.DateTime): Checked<string> =>
  attempt("format_intl", () => {
    const { locale, ...options } = intl(o);
    return DateTime.formatIntl(self, new Intl.DateTimeFormat(locale, options));
  });

/** `DateTime.json_utc`: ISO 8601 text, decoded to UTC. */
export const flickJsonUtc: AnySchema = /*#__PURE__*/ fallible(Schema.DateTimeUtcFromString);

/** `DateTime.json_utc_millis`: milliseconds since the epoch, a whole JSON number. */
export const flickJsonUtcMillis: AnySchema = /*#__PURE__*/ fallible(Schema.DateTimeUtcFromMillis);

/** `DateTime.json_zoned`: ISO 8601 text with the zone, `…+01:00[Europe/Paris]`. */
export const flickJsonZoned: AnySchema = /*#__PURE__*/ fallible(Schema.DateTimeZonedFromString);

/** `DateTime.json_zone`: an IANA name or an offset `+02:00`. */
export const flickJsonZone: AnySchema = /*#__PURE__*/ fallible(Schema.TimeZoneFromString);

/** `DateTime.json_zone_named`: an IANA name only. */
export const flickJsonZoneNamed: AnySchema = /*#__PURE__*/ fallible(Schema.TimeZoneNamedFromString);

/** `DateTime.json_zone_offset`: a fixed offset as a JSON number of milliseconds. */
export const flickJsonZoneOffset: AnySchema = /*#__PURE__*/ jsonNumber.pipe(
  Schema.decodeTo(Schema.TimeZoneOffset, SchemaTransformation.timeZoneOffsetFromNumber),
);
