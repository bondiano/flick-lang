// RegExp operations of `String`, beside the plain string helpers.
import { none, some, type Option } from "../../core/data.ts";

export * from "../../core/String.ts";
export const ReducerConcat = { initial: "", combine: (a: string, b: string): string => a + b };
export const search = (expression: RegExp) => (text: string): Option<number> => {
  const index = text.search(expression);
  return index < 0 ? none : some(index);
};
export const replace = (expression: RegExp, replacement: string) => (text: string): string =>
  text.replace(expression, replacement);
