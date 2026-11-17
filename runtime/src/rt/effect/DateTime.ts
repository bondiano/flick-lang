// The instant part of DateTime on Flick's own runtime.

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

/** `DateTime.unsafe_from_epoch_millis`: unchecked, as a clock reads a valid time. */
export const flickUnsafeFromEpochMillis = (millis: number): DateTime => dateTimeHooks.makeUtc(millis);
export const toEpochMillis = (value: DateTime): number => value.epochMilliseconds;
