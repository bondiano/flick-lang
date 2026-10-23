// Effect's `Record` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Record } from "effect";

export * from "effect/Record";

/** `Record.make_equivalence`: the equivalence curried, as Flick calls a function. */
export const flickMakeEquivalence =
  <A>(same: (a: A, b: A) => boolean) =>
  (self: Record.ReadonlyRecord<string, A>) =>
  (that: Record.ReadonlyRecord<string, A>): boolean =>
    Record.makeEquivalence(same)(self, that);
