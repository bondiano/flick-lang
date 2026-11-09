// Effect's `Formatter` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { Formatter } from "effect";

export * from "effect/Formatter";

/** `Formatter.format_json indent data`: 0 is one line. */
export const flickFormatJson = (indent: number, data: unknown): string =>
  indent > 0 ? Formatter.formatJson(data, { space: indent }) : Formatter.formatJson(data);
