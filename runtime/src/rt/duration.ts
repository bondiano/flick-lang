// Durations, in milliseconds, and the wait for one.
import { type Result, err, ok } from "../core/data.ts";
import { tag } from "../tags.ts";
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

/** spike: a duration may be Effect's: the one the bridge gives Flick, for Effect's code to read. */
export const durationHooks: { millis: (d: unknown) => number } = { millis: (d) => (d as Duration).millis };

/** A duration that is not positive yields; an infinite one never ends. */
export const sleep = (d: Duration): Instruction =>
  new Suspend((resolve) => {
    const millis = durationHooks.millis(d);
    if (millis <= 0) {
      schedule(() => resolve(undefined));
      return undefined;
    }
    if (millis === Infinity) return undefined;
    const timer = setTimeout(() => resolve(undefined), millis);
    return () => clearTimeout(timer);
  });
