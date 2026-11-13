// The instant part of DateTime on Flick's own runtime. It reads the fiber's
// clock, including Test.clock_test's virtual clock.
import { clock } from "../clock.ts";

export interface DateTime {
  readonly epochMilliseconds: number;
}

/**
 * The Effect bridge keeps the native clock while returning its DateTime value.
 * `toDate` is the wall-clock time in the DateTime's zone as the UTC fields of
 * a Date, as Effect's `toDate`; only the bridge makes a zoned DateTime.
 */
export const dateTimeHooks: { makeUtc: (millis: number) => DateTime; toDate: (value: DateTime) => Date } = {
  makeUtc: (epochMilliseconds) => Object.freeze({ epochMilliseconds }),
  toDate: (value) => new Date(value.epochMilliseconds),
};

export const flickNow = (): DateTime => dateTimeHooks.makeUtc(clock().millis());
export const toEpochMillis = (value: DateTime): number => value.epochMilliseconds;
