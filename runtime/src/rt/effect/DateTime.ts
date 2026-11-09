// The instant part of DateTime on Flick's own runtime. It reads the fiber's
// clock, including Test.clock_test's virtual clock.
import { clock } from "../clock.ts";

export interface DateTime {
  readonly epochMilliseconds: number;
}

export const flickNow = (): DateTime => Object.freeze({ epochMilliseconds: clock().millis() });
export const toEpochMillis = (value: DateTime): number => value.epochMilliseconds;
