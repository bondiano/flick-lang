// Effect's `Duration` for the bindings of the standard library, and the
// adapters that keep Flick durations checked and never negative.
import { Duration, Option, type Result } from "effect";

import { none, ok, some } from "../data.ts";
import { type DurationError, checkedDuration, durationError } from "../stdlib/decimal.ts";

export * from "effect/Duration";

type Checked = Result.Result<Duration.Duration, DurationError>;

/** A computed duration, or an error when it came out negative. */
const nonNegative = (what: string, d: Duration.Duration): Checked =>
  Duration.isNegative(d) ? durationError(`${what}: the result is negative`) : ok(d);

export const flickMicros = (n: bigint): Checked =>
  n < 0n ? durationError(`micros: ${n} is negative`) : ok(Duration.micros(n));
export const flickHours = (n: number): Checked => checkedDuration("hours", n, 3_600_000);
export const flickDays = (n: number): Checked => checkedDuration("days", n, 86_400_000);
export const flickWeeks = (n: number): Checked => checkedDuration("weeks", n, 604_800_000);

/** The string forms of `Duration.Input` have a unit after a space; `fromInput` checks the rest. */
const isInput = (s: string): s is Extract<Duration.Input, string> => s === "Infinity" || s.includes(" ");

/** `Duration.from_input`: the string forms of `Duration.Input`. */
export const flickFromInput = (s: string): Checked => {
  const parsed = isInput(s) ? Duration.fromInput(s) : Option.none();
  return Option.isSome(parsed) ? nonNegative(`from_input ${JSON.stringify(s)}`, parsed.value) : durationError(`not a duration: ${JSON.stringify(s)}`);
};

const finite =
  (to: (d: Duration.Duration) => number) =>
  (d: Duration.Duration): Option.Option<number> =>
    Duration.isFinite(d) ? some(to(d)) : none;

export const flickToSeconds = /*#__PURE__*/ finite(Duration.toSeconds);
export const flickToMinutes = /*#__PURE__*/ finite(Duration.toMinutes);
export const flickToHours = /*#__PURE__*/ finite(Duration.toHours);
export const flickToDays = /*#__PURE__*/ finite(Duration.toDays);
export const flickToWeeks = /*#__PURE__*/ finite(Duration.toWeeks);

export const flickToHrTime = (d: Duration.Duration): Option.Option<readonly [number, number]> =>
  Duration.isFinite(d) ? some(Duration.toHrTime(d)) : none;

/** `d |> Duration.subtract x`. */
export const flickSubtract =
  (that: Duration.Duration) =>
  (self: Duration.Duration): Checked =>
    nonNegative("subtract", Duration.subtract(self, that));

/** `d |> Duration.times n`. */
export const flickTimes =
  (n: number) =>
  (self: Duration.Duration): Checked =>
    Number.isNaN(n) || n < 0 ? durationError(`times: ${n} is negative or NaN`) : nonNegative("times", Duration.times(self, n));

/** `d |> Duration.divide n`. */
export const flickDivide =
  (n: number) =>
  (self: Duration.Duration): Checked => {
    const q = n < 0 ? Option.none() : Duration.divide(self, n);
    return Option.isSome(q) ? nonNegative("divide", q.value) : durationError(`divide: cannot divide by ${n}`);
  };

/** `Duration.match_pair`: the callbacks of a Flick record come curried. */
export const flickMatchPair =
  <A>(
    that: Duration.Duration,
    options: {
      readonly onMillis: (self: number) => (that: number) => A;
      readonly onNanos: (self: bigint) => (that: bigint) => A;
      readonly onInfinity: (self: Duration.Duration) => (that: Duration.Duration) => A;
    },
  ) =>
  (self: Duration.Duration): A =>
    Duration.matchPair(self, that, {
      onMillis: (a, b) => options.onMillis(a)(b),
      onNanos: (a, b) => options.onNanos(a)(b),
      onInfinity: (a, b) => options.onInfinity(a)(b),
    });

const saturate = (n: number): number => Math.min(Math.max(n, -0x80000000), 0x7fffffff);

/** `Duration.parts` as Flick `Int`s: a part beyond 32 bits, as those of `infinity` are, saturates. */
export const parts = (self: Duration.Duration): ReturnType<typeof Duration.parts> => {
  const p = Duration.parts(self);
  return {
    days: saturate(p.days),
    hours: saturate(p.hours),
    minutes: saturate(p.minutes),
    seconds: saturate(p.seconds),
    millis: saturate(p.millis),
    nanos: saturate(p.nanos),
  };
};
