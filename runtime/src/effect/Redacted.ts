// Effect's `Redacted` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Redacted } from "effect";

export * from "effect/Redacted";

/** `Redacted.equivalent is_equivalent x y`. */
export const flickEquivalent = <A>(
  isEquivalent: (self: A, that: A) => boolean,
  self: Redacted.Redacted<A>,
  that: Redacted.Redacted<A>,
): boolean => Redacted.makeEquivalence(isEquivalent)(self, that);
