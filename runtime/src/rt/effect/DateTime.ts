// The instant part of DateTime on Flick's own runtime. It reads the fiber's
// clock, including Test.clock_test's virtual clock.
import { clock } from "../clock.ts";

export interface DateTime {
  readonly epochMilliseconds: number;
}

/** The Effect bridge keeps the native clock while returning its DateTime value. */
export const dateTimeHooks: { makeUtc: (millis: number) => DateTime } = {
  makeUtc: (epochMilliseconds) => Object.freeze({ epochMilliseconds }),
};

export const flickNow = (): DateTime => dateTimeHooks.makeUtc(clock().millis());
export const toEpochMillis = (value: DateTime): number => value.epochMilliseconds;
