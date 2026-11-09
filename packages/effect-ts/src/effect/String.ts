// Effect's `String` for the bindings of the standard library, and the
// adapters where a Flick value differs from what Effect takes.
import { String as Str } from "effect";

import type { Ordering } from "../data.ts";
import { constructorName } from "@flick-lang/runtime/tags";
import { toOrdering } from "./ordering.ts";

export * from "effect/String";

/** `String.locale_compare locale that self`. */
export const flickLocaleCompare = (locale: string, that: string, self: string): Ordering =>
  toOrdering(Str.localeCompare(that, [locale])(self));

type Form = "NFC" | "NFD" | "NFKC" | "NFKD";

const isForm = (name: string): name is Form => name === "NFC" || name === "NFD" || name === "NFKC" || name === "NFKD";

/** `String.normalize form self`: the form is a `String.NormalForm`. */
export const flickNormalize = (form: Readonly<{ _tag: string }>, self: string): string => {
  const name = constructorName(form._tag);
  return isForm(name) ? Str.normalize(name)(self) : self;
};

/** `String.lines`: the lines of `linesIterator` as a list. */
export const flickLines = (self: string): Array<string> => Array.from(Str.linesIterator(self));

/** `String.lines_with_separators`. */
export const flickLinesWithSeparators = (self: string): Array<string> => Array.from(Str.linesWithSeparators(self));
