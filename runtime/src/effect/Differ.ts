// Effect's `Differ` for the bindings of the standard library: a differ's
// methods as functions of the differ, which Flick passes last.
import type { Differ } from "effect";

export * from "effect/Differ";

export const flickEmpty = <T, P>(differ: Differ.Differ<T, P>): P => differ.empty;
export const flickDiff = <T, P>(oldValue: T, newValue: T, differ: Differ.Differ<T, P>): P => differ.diff(oldValue, newValue);
export const flickCombine = <T, P>(first: P, second: P, differ: Differ.Differ<T, P>): P => differ.combine(first, second);
export const flickPatch = <T, P>(oldValue: T, patch: P, differ: Differ.Differ<T, P>): T => differ.patch(oldValue, patch);
