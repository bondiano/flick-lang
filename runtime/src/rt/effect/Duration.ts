// Checked duration conversions and arithmetic over the own runtime's value.
import { GT, LT, err, none, ok, some, type Option, type Result } from "../../core/data.ts";
import {
  Duration, durationDays, durationHours, durationInfinity,
  durationWeeks, durationFormat, durationCompare, exactNanos,
} from "../duration.ts";
import { tag } from "../../tags.ts";

interface DurationError { readonly _tag: string; readonly _0: string }
type Checked = Result<Duration, DurationError>;
const error = (message: string): Checked => err({ _tag: tag("Duration.DurationError.DurationError"), _0: message });
const finite = (duration: Duration): boolean => Number.isFinite(duration.millis);
const nanos = exactNanos;
const fromNanos = (value: bigint): Duration => new Duration(Number(value) / 1_000_000, value);

export const flickHours = durationHours;
export const flickDays = durationDays;
export const flickWeeks = durationWeeks;
export const flickMicros = (value: bigint): Checked =>
  value < 0n ? error(`micros: ${value} is negative`) : ok(fromNanos(value * 1000n));
const inputUnits: Readonly<Record<string, number>> = {
  nanosecond: 0.000001, nanoseconds: 0.000001, microsecond: 0.001, microseconds: 0.001,
  millisecond: 1, milliseconds: 1, millis: 1, second: 1000, seconds: 1000,
  minute: 60_000, minutes: 60_000, hour: 3_600_000, hours: 3_600_000,
  day: 86_400_000, days: 86_400_000, week: 604_800_000, weeks: 604_800_000,
};
export const flickFromInput = (text: string): Checked => {
  if (text === "Infinity") return ok(durationInfinity);
  const match = /^([+-]?\d+(?:\.\d+)?) ([A-Za-z]+)$/.exec(text);
  const unit = match ? inputUnits[match[2] ?? ""] : undefined;
  if (!match || unit === undefined) return error(`not a duration: ${JSON.stringify(text)}`);
  const value = Number(match[1]);
  if (value < 0) return error(`from_input ${JSON.stringify(text)}: the result is negative`);
  const millis = value * unit;
  return Number.isFinite(millis) ? ok(new Duration(millis)) : error(`not a duration: ${JSON.stringify(text)}`);
};

export const isZero = (d: Duration): boolean => d.millis === 0;
export const isPositive = (d: Duration): boolean => d.millis > 0;
const convert = (divisor: number) => (duration: Duration): Option<number> =>
  finite(duration) ? some(duration.millis / divisor) : none;
export const flickToSeconds = convert(1000);
export const flickToMinutes = convert(60_000);
export const flickToHours = convert(3_600_000);
export const flickToDays = convert(86_400_000);
export const flickToWeeks = convert(604_800_000);
export const toNanos = (duration: Duration): Option<bigint> => finite(duration) ? some(nanos(duration)) : none;
export const flickToHrTime = (duration: Duration): Option<readonly [number, number]> => {
  if (!finite(duration)) return none;
  const total = nanos(duration);
  return some([Number(total / 1_000_000_000n), Number(total % 1_000_000_000n)]);
};
const saturate = (value: bigint): number => Number(value > 0x7fffffffn ? 0x7fffffffn : value);
export const parts = (duration: Duration): {
  readonly days: number; readonly hours: number; readonly minutes: number;
  readonly seconds: number; readonly millis: number; readonly nanos: number;
} => {
  if (!finite(duration)) return { days: 0x7fffffff, hours: 0x7fffffff, minutes: 0x7fffffff, seconds: 0x7fffffff, millis: 0x7fffffff, nanos: 0x7fffffff };
  let left = nanos(duration);
  const take = (unit: bigint): number => { const count = left / unit; left %= unit; return saturate(count); };
  return {
    days: take(86_400_000_000_000n), hours: take(3_600_000_000_000n),
    minutes: take(60_000_000_000n), seconds: take(1_000_000_000n),
    millis: take(1_000_000n), nanos: take(1n),
  };
};
interface Match<A> { readonly onMillis: (value: number) => A; readonly onNanos: (value: bigint) => A; readonly onInfinity: () => A }
export const match = <A>(options: Match<A>) => (duration: Duration): A =>
  !finite(duration) ? options.onInfinity() : duration.nanos === undefined ? options.onMillis(duration.millis) : options.onNanos(duration.nanos);
interface MatchPair<A> {
  readonly onMillis: (a: number) => (b: number) => A;
  readonly onNanos: (a: bigint) => (b: bigint) => A;
  readonly onInfinity: (a: Duration) => (b: Duration) => A;
}
export const flickMatchPair = <A>(that: Duration, options: MatchPair<A>) => (self: Duration): A =>
  !finite(self) || !finite(that) ? options.onInfinity(self)(that)
    : self.nanos !== undefined || that.nanos !== undefined
      ? options.onNanos(nanos(self))(nanos(that)) : options.onMillis(self.millis)(that.millis);
export const flickSubtract = (that: Duration) => (self: Duration): Checked => {
  if (durationCompare(self, that) === LT) return error("subtract: the result is negative");
  return self.nanos !== undefined || that.nanos !== undefined
    ? ok(fromNanos(nanos(self) - nanos(that))) : ok(new Duration(self.millis - that.millis));
};
export const flickTimes = (factor: number) => (self: Duration): Checked => {
  if (Number.isNaN(factor) || factor < 0) return error(`times: ${factor} is negative or NaN`);
  if (!finite(self) || factor === Infinity) return ok(durationInfinity);
  return ok(new Duration(self.millis * factor));
};
export const flickDivide = (divisor: number) => (self: Duration): Checked => {
  if (!Number.isFinite(divisor) || divisor <= 0) return error(`divide: cannot divide by ${divisor}`);
  return ok(new Duration(self.millis / divisor));
};
export const min = (that: Duration) => (self: Duration): Duration => durationCompare(self, that) !== GT ? self : that;
export const max = (that: Duration) => (self: Duration): Duration => durationCompare(self, that) !== LT ? self : that;
interface Bounds { readonly minimum: Duration; readonly maximum: Duration }
export const clamp = (bounds: Bounds) => (self: Duration): Duration =>
  durationCompare(self, bounds.minimum) === LT ? bounds.minimum
    : durationCompare(self, bounds.maximum) === GT ? bounds.maximum : self;
export const between = (bounds: Bounds) => (self: Duration): boolean =>
  durationCompare(self, bounds.minimum) !== LT && durationCompare(self, bounds.maximum) !== GT;
export const isLessThan = (that: Duration) => (self: Duration): boolean => durationCompare(self, that) === LT;
export const isLessThanOrEqualTo = (that: Duration) => (self: Duration): boolean => durationCompare(self, that) !== GT;
export const isGreaterThan = (that: Duration) => (self: Duration): boolean => durationCompare(self, that) === GT;
export const isGreaterThanOrEqualTo = (that: Duration) => (self: Duration): boolean => durationCompare(self, that) !== LT;
export { durationFormat as format };
