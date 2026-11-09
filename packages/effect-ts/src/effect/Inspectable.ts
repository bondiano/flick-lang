// Effect's `Inspectable` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Inspectable } from "effect";

export * from "effect/Inspectable";

/** `Inspectable.to_string_unknown indent data`: 0 is one line, as `JSON.stringify` takes it. */
export const flickToStringUnknown = (indent: number, data: unknown): string =>
  Inspectable.toStringUnknown(data, indent);
