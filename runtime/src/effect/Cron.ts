// Effect's `Cron` for the bindings of the standard library, and the adapters
// where a Flick value differs: `Option` fields, `DateTime` instead of `Date`,
// and `Cron.CronParseError` instead of Effect's error class or a throw.
import { Cron, DateTime, Option, Result } from "effect";

import { type Tagged, err, none, ok, some } from "../data.ts";
import { tag } from "../tags.ts";

export * from "effect/Cron";

const ERROR = tag("Cron.CronParseError.CronParseError");

/** `Cron.CronParseError`. */
export type CronParseError = Tagged<typeof ERROR, string>;

const fail = (message: string): Result.Result<never, CronParseError> => err({ _tag: ERROR, _0: message });

interface Fields {
  readonly seconds: Option.Option<ReadonlyArray<number>>;
  readonly minutes: ReadonlyArray<number>;
  readonly hours: ReadonlyArray<number>;
  readonly days: ReadonlyArray<number>;
  readonly months: ReadonlyArray<number>;
  readonly weekdays: ReadonlyArray<number>;
  readonly and: Option.Option<boolean>;
  readonly tz: DateTime.TimeZone;
}

export const flickMake = (f: Fields): Result.Result<Cron.Cron, CronParseError> => {
  try {
    return ok(
      Cron.make({
        ...f,
        seconds: Option.getOrUndefined(f.seconds),
        and: Option.getOrUndefined(f.and),
      }),
    );
  } catch (e) {
    return fail(e instanceof Error ? e.message : String(e));
  }
};

export const flickParse = (tz: DateTime.TimeZone, s: string): Result.Result<Cron.Cron, CronParseError> =>
  Result.mapError(Cron.parse(s, tz), (e) => ({ _tag: ERROR, _0: `${e.message}: ${JSON.stringify(s)}` }));

export const flickFormat = (options: { readonly includeSeconds: Option.Option<boolean> }, cron: Cron.Cron): string =>
  Cron.format(cron, { includeSeconds: Option.getOrUndefined(options.includeSeconds) });

export const flickMatch = (now: DateTime.DateTime, cron: Cron.Cron): boolean => Cron.match(cron, now);

/** A date of `find`, or `None` when Effect finds none and throws. */
const found = (find: () => Date): Option.Option<DateTime.DateTime> => {
  try {
    return some(DateTime.fromDateUnsafe(find()));
  } catch {
    return none;
  }
};

export const flickNext = (now: DateTime.DateTime, cron: Cron.Cron): Option.Option<DateTime.DateTime> => found(() => Cron.next(cron, now));
export const flickPrev = (now: DateTime.DateTime, cron: Cron.Cron): Option.Option<DateTime.DateTime> => found(() => Cron.prev(cron, now));

export const flickSequence = (n: number, now: DateTime.DateTime, cron: Cron.Cron): ReadonlyArray<DateTime.DateTime> => {
  const out: Array<DateTime.DateTime> = [];
  if (n <= 0) return out;
  try {
    for (const date of Cron.sequence(cron, now)) {
      out.push(DateTime.fromDateUnsafe(date));
      if (out.length >= n) break;
    }
  } catch {
    // no further matching date: the list ends here
  }
  return out;
};
