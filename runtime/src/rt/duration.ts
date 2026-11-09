// Durations, in milliseconds, and the wait for one.
import { type Option, type Result, err, none, ok, some } from "../core/data.ts";
import { tag } from "../tags.ts";
import { clock } from "./clock.ts";
import { type Instruction, Suspend, schedule } from "./fiber.ts";

/** `Duration`. */
export class Duration {
  readonly millis: number;
  constructor(millis: number) {
    this.millis = millis;
  }
}

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
export const durationHours = (n: number) => checked("hours", n, 3_600_000);
export const durationDays = (n: number) => checked("days", n, 86_400_000);
export const durationWeeks = (n: number) => checked("weeks", n, 604_800_000);

export const durationZero = /*#__PURE__*/ new Duration(0);
export const durationInfinity = /*#__PURE__*/ new Duration(Infinity);
export const durationIsFinite = (d: Duration): boolean => Number.isFinite(d.millis);
export const durationToMillis = (d: Duration): Option<number> => (Number.isFinite(d.millis) ? some(d.millis) : none);
export const durationAdd = (a: Duration, b: Duration): Duration => new Duration(a.millis + b.millis);

/** A wait on the clock of the fiber: a duration that is not positive yields; an infinite one never ends. */
export const sleep = (d: Duration): Instruction =>
  new Suspend((resolve) => {
    if (d.millis <= 0) {
      schedule(() => resolve(undefined));
      return undefined;
    }
    if (d.millis === Infinity) return undefined;
    return clock().timer(d.millis, () => resolve(undefined));
  });
