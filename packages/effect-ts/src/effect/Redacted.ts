// Effect's `Redacted` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import type { Redacted } from "effect";
import type { Redacted as NativeRedacted } from "@flick-lang/runtime/rt/redacted";
import { redactedReveal } from "../stdlib/misc.ts";

export * from "effect/Redacted";

/** `Redacted.equivalent is_equivalent x y`. */
export const flickEquivalent = <A>(
  isEquivalent: (self: A, that: A) => boolean,
  self: Redacted.Redacted<A> | NativeRedacted<A>,
  that: Redacted.Redacted<A> | NativeRedacted<A>,
): boolean => isEquivalent(redactedReveal(self), redactedReveal(that));
