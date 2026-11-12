// Durations, in milliseconds, and the wait for one.
import { EQ, GT, LT, type Option, type Result, type Ordering, err, hash, none, ok, some, runtimeEquals, runtimeHash } from "../core/data.ts";
import { tag } from "../tags.ts";
import { clock } from "./clock.ts";
import { type Instruction, Suspend, schedule } from "./fiber.ts";

/** `Duration`. */
export class Duration {
  readonly millis: number;
  readonly nanos: bigint | undefined;
  constructor(millis: number, nanos?: bigint) {
    this.millis = millis;
    this.nanos = nanos;
  }
  [runtimeEquals](other: unknown): boolean {
    if (!(other instanceof Duration)) return false;
    if (this.nanos === undefined && other.nanos === undefined) return this.millis === other.millis;
    if (!Number.isFinite(this.millis) || !Number.isFinite(other.millis)) return this.millis === other.millis;
    return exactNanos(this) === exactNanos(other);
  }
  [runtimeHash](): number { return hash(this.millis); }
}

/** Accepts a host duration when the optional Effect bridge is installed. */
export const durationHooks: { millis: (duration: unknown) => number; native: (duration: unknown) => Duration } = {
  millis: (duration) => {
    if (duration instanceof Duration) return duration.millis;
    throw new TypeError("not a duration of this runtime");
  },
  native: (duration) => {
    if (duration instanceof Duration) return duration;
    throw new TypeError("not a duration of this runtime");
  },
};

const DURATION = /*#__PURE__*/ tag("Duration.DurationError.DurationError");

const error = (message: string) => err({ _tag: DURATION, _0: message });

const checked = (unit: string, n: number, millisPer: number): Result<Duration, Readonly<{ _tag: string; _0: string }>> => {
  if (Number.isNaN(n)) return error(`${unit}: NaN is not a duration`);
  if (n < 0) return error(`${unit}: ${n} is negative`);
  const millis = n * millisPer;
  if (!Number.isFinite(millis)) return error(`${unit}: ${n} overflows; use Duration.infinity for an unbounded duration`);
  return ok(new Duration(millis));
};

export const durationMillis = (n: number) => checked("millis", n, 1);
export const durationSeconds = (n: number) => checked("seconds", n, 1000);
export const durationMinutes = (n: number) => checked("minutes", n, 60_000);
export const durationNanos = (n: bigint): Result<Duration, Readonly<{ _tag: string; _0: string }>> =>
  n < 0n ? error(`nanos: ${n} is negative`) : ok(new Duration(Number(n) / 1_000_000, n));
export const exactNanos = (duration: Duration): bigint =>
  duration.nanos ?? BigInt(Math.round(duration.millis * 1_000_000));
export const durationHours = (n: number) => checked("hours", n, 3_600_000);
export const durationDays = (n: number) => checked("days", n, 86_400_000);
export const durationWeeks = (n: number) => checked("weeks", n, 604_800_000);

export const durationZero = /*#__PURE__*/ new Duration(0);
export const durationInfinity = /*#__PURE__*/ new Duration(Infinity);
export const durationIsFinite = (d: Duration): boolean => Number.isFinite(d.millis);
export const durationToMillis = (d: Duration): Option<number> => (Number.isFinite(d.millis) ? some(d.millis) : none);
export const durationAdd = (a: Duration, b: Duration): Duration =>
  !Number.isFinite(a.millis) || !Number.isFinite(b.millis) ? durationInfinity
    : a.nanos !== undefined || b.nanos !== undefined
    ? new Duration(a.millis + b.millis, exactNanos(a) + exactNanos(b))
    : new Duration(a.millis + b.millis);
export const durationCompare = (a: Duration, b: Duration): Ordering => {
  if ((a.nanos !== undefined || b.nanos !== undefined) && Number.isFinite(a.millis) && Number.isFinite(b.millis)) {
    const x = exactNanos(a);
    const y = exactNanos(b);
    return x < y ? LT : x > y ? GT : EQ;
  }
  return a.millis < b.millis ? LT : a.millis > b.millis ? GT : EQ;
};
/** `Duration.format`, as Effect's: a duration with nanoseconds is finite however large, and its days may print as `Infinity`. */
export const durationFormat = (input: unknown): string => {
  const duration = durationHooks.native(input);
  if (duration.nanos === undefined && !Number.isFinite(duration.millis)) return duration.millis > 0 ? "Infinity" : "-Infinity";
  const nanos = exactNanos(duration);
  if (nanos === 0n) return "0";
  if (nanos < 0n) return `-${durationFormat(new Duration(-duration.millis, -nanos))}`;
  const ms = nanos / 1_000_000n;
  const sec = ms / 1000n;
  const min = sec / 60n;
  const hr = min / 60n;
  const parts: ReadonlyArray<readonly [number, string]> = [
    [Number(hr / 24n), "d"], [Number(hr % 24n), "h"], [Number(min % 60n), "m"],
    [Number(sec % 60n), "s"], [Number(ms % 1000n), "ms"], [Number(nanos % 1_000_000n), "ns"],
  ];
  return parts.filter(([value]) => value !== 0).map(([value, unit]) => `${value}${unit}`).join(" ");
};

/** A wait on the clock of the fiber: a duration that is not positive yields; an infinite one never ends. */
export const sleep = (d: Duration): Instruction =>
  new Suspend((resolve) => {
    const millis = durationHooks.millis(d);
    if (millis <= 0) {
      schedule(() => resolve(undefined));
      return undefined;
    }
    if (millis === Infinity) return undefined;
    return clock().timer(millis, () => resolve(undefined));
  });
