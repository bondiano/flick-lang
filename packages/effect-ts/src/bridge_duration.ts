// Pure duration conversions shared by the bridge and its runtime hooks.
import { Duration as EDuration, Option as EOption } from "effect";
import { Duration } from "@flick-lang/runtime/rt/duration";

export const durationToEffect = (value: unknown): unknown =>
  value instanceof Duration
    ? value.nanos === undefined ? EDuration.millis(value.millis) : EDuration.nanos(value.nanos)
    : value;

export const durationFromEffect = (value: unknown): unknown => {
  if (!EDuration.isDuration(value)) return value;
  const nanos = EDuration.toNanos(value);
  return EOption.isSome(nanos) ? new Duration(EDuration.toMillis(value), nanos.value) : new Duration(Infinity);
};

export const durationMillis = (d: unknown): number => {
  if (d instanceof Duration) return d.millis;
  if (EDuration.isDuration(d)) return EDuration.toMillis(d);
  throw new TypeError("not a duration of this runtime or of Effect");
};
export const durationNative = (d: unknown): Duration => {
  const converted = durationFromEffect(d);
  if (converted instanceof Duration) return converted;
  throw new TypeError("not a duration of this runtime or of Effect");
};
