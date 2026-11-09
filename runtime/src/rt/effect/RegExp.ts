// Regular expressions are immutable from Flick: operations clone the pattern
// when a global flag is needed, so no call observes or mutates lastIndex.
import { err, none, ok, some, type Option, type Result } from "../../core/data.ts";
import { tag } from "../../tags.ts";

const REGEXP_ERROR = /*#__PURE__*/ tag("RegExp.RegExpError.RegExpError");
type RegExpError = { readonly _tag: string; readonly _0: string };
interface Match { readonly matched: string; readonly index: number; readonly captures: ReadonlyArray<Option<string>> }
const copy = (expression: RegExp, global: boolean): RegExp =>
  new RegExp(expression.source, global ? expression.flags.replace("g", "") + "g" : expression.flags.replace("g", ""));
const matched = (match: RegExpMatchArray): Match => ({
  matched: match[0] ?? "", index: match.index ?? 0,
  captures: match.slice(1).map((capture) => capture === undefined ? none : some(capture)),
});

export const flickMake = (pattern: string, flags: string): Result<RegExp, RegExpError> => {
  if (flags.includes("y")) return err({ _tag: REGEXP_ERROR, _0: "the sticky flag `y` is not supported" });
  try { return ok(new RegExp(pattern, flags)); }
  catch (error) { return err({ _tag: REGEXP_ERROR, _0: error instanceof Error ? error.message : String(error) }); }
};
export const escape = (text: string): string => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
export const flickMatch = (expression: RegExp, text: string): Option<Match> => {
  const result = text.match(copy(expression, false));
  return result === null ? none : some(matched(result));
};
export const flickMatchAll = (expression: RegExp, text: string): Array<Match> =>
  Array.from(text.matchAll(copy(expression, true)), matched);
export const flickReplaceAll = (expression: RegExp, replacement: string, text: string): string =>
  text.replace(copy(expression, true), replacement);
