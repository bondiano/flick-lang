// Effect's `RegExp` for the bindings of the standard library, and the
// adapters of `RegExp`: the regular-expression operations of Effect's
// `String`. A Flick regular expression never has the sticky flag, and the
// operations never read `lastIndex`, so they are pure.
import { Option, RegExp as Re, type Result, String as Str } from "effect";

import { type Tagged, err, ok } from "../data.ts";
import { tag } from "../tags.ts";

export * from "effect/RegExp";

const REGEXP_ERROR = /*#__PURE__*/ tag("RegExp.RegExpError.RegExpError");

/** `RegExp.RegExpError`. */
export type RegExpError = Tagged<typeof REGEXP_ERROR, string>;

/** `RegExp.make pattern flags`: a syntax error or the sticky flag is an error. */
export const flickMake = (pattern: string, flags: string): Result.Result<RegExp, RegExpError> => {
  if (flags.includes("y")) return err({ _tag: REGEXP_ERROR, _0: "the sticky flag `y` is not supported" });
  try {
    return ok(new Re.RegExp(pattern, flags));
  } catch (e) {
    return err({ _tag: REGEXP_ERROR, _0: e instanceof Error ? e.message : String(e) });
  }
};

/** `RegExp.Match`. */
export interface Match {
  readonly matched: string;
  readonly index: number;
  readonly captures: ReadonlyArray<Option.Option<string>>;
}

const toMatch = (m: RegExpMatchArray): Match => ({
  matched: m[0],
  index: m.index ?? 0,
  captures: m.slice(1).map((c) => Option.fromUndefinedOr(c)),
});

const withFlag = (re: RegExp, global: boolean): RegExp =>
  re.global === global ? re : new RegExp(re.source, global ? `${re.flags}g` : re.flags.replace("g", ""));

/** `RegExp.match re self`: the first match, by a copy without the global flag. */
export const flickMatch = (re: RegExp, self: string): Option.Option<Match> =>
  Option.map(Str.match(withFlag(re, false))(self), toMatch);

/** `RegExp.match_all re self`: every match, by a copy with the global flag. */
export const flickMatchAll = (re: RegExp, self: string): Array<Match> =>
  Array.from(Str.matchAll(withFlag(re, true))(self), toMatch);

/** `RegExp.replace_all re replacement self`, by a copy with the global flag. */
export const flickReplaceAll = (re: RegExp, replacement: string, self: string): string =>
  Str.replaceAll(withFlag(re, true), replacement)(self);
