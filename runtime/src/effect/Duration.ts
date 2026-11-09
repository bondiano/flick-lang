// The bindings of `Duration` on the kernel that a duration of milliseconds has:
// the rest is not on Flick's own runtime yet.
import { type Duration, durationDays, durationHours, durationWeeks } from "../duration.ts";

export const flickHours = durationHours;
export const flickDays = durationDays;
export const flickWeeks = durationWeeks;

export const isZero = (d: Duration): boolean => d.millis === 0;
export const isPositive = (d: Duration): boolean => d.millis > 0;
