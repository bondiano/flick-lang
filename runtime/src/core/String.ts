// The `String` bindings of the standard library without Effect: the
// semantics of `effect/String` (4.1.2) on plain data, curried as the
// bindings call them.
import { constructorName } from "../tags.ts";
import { EQ, GT, LT, fromNullable, none, type Option, type Ordering, some } from "./data.ts";

/** Effect's `Count.normalize`: a whole count, 0 for a negative one or NaN. */
const count = (n: number): number => (n > 0 ? Math.floor(n) : 0);

export const trim = (self: string): string => self.trim();
export const trimStart = (self: string): string => self.trimStart();
export const trimEnd = (self: string): string => self.trimEnd();
export const length = (self: string): number => self.length;
export const isNonEmpty = (self: string): boolean => self.length > 0;

/** `"".split("")` is `[]`; Effect gives `[self]` then. */
export const split =
  (separator: string) =>
  (self: string): Array<string> => {
    const out = self.split(separator);
    return out.length > 0 ? out : [self];
  };

export const includes = (part: string) => (self: string) => self.includes(part);
export const startsWith = (prefix: string) => (self: string) => self.startsWith(prefix);
export const endsWith = (suffix: string) => (self: string) => self.endsWith(suffix);

const found = (i: number): Option<number> => (i >= 0 ? some(i) : none);
export const indexOf = (part: string) => (self: string) => found(self.indexOf(part));
export const lastIndexOf = (part: string) => (self: string) => found(self.lastIndexOf(part));

export const toUpperCase = (self: string): string => self.toUpperCase();
export const toLowerCase = (self: string): string => self.toLowerCase();
export const toLocaleUpperCase = (locale: string) => (self: string) => self.toLocaleUpperCase(locale);
export const toLocaleLowerCase = (locale: string) => (self: string) => self.toLocaleLowerCase(locale);

/** The first code unit, not code point, as Effect does. */
export const capitalize = (self: string): string => (self.length === 0 ? self : self.charAt(0).toUpperCase() + self.slice(1));
export const uncapitalize = (self: string): string =>
  self.length === 0 ? self : self.charAt(0).toLowerCase() + self.slice(1);

export const slice = (start: number, end: number) => (self: string) => self.slice(start, end);
export const substring = (start: number, end: number) => (self: string) => self.substring(start, end);
export const takeLeft = (n: number) => (self: string) => self.slice(0, count(n));
export const takeRight = (n: number) => (self: string) => self.slice(self.length - Math.min(count(n), self.length));

export const at = (index: number) => (self: string) => fromNullable(self.at(index));
export const charAt = (index: number) => (self: string) => {
  const c = self.charAt(index);
  return c.length > 0 ? some(c) : none;
};
export const charCodeAt = (index: number) => (self: string) => {
  const c = self.charCodeAt(index);
  return Number.isNaN(c) ? none : some(c);
};
export const codePointAt = (index: number) => (self: string) => fromNullable(self.codePointAt(index));

/** `String.locale_compare locale that self`. */
export const flickLocaleCompare = (locale: string, that: string, self: string): Ordering => {
  const n = self.localeCompare(that, [locale]);
  return n < 0 ? LT : n > 0 ? GT : EQ;
};

type Form = "NFC" | "NFD" | "NFKC" | "NFKD";

const isForm = (name: string): name is Form => name === "NFC" || name === "NFD" || name === "NFKC" || name === "NFKD";

/** `String.normalize form self`: the form is a `String.NormalForm`. */
export const flickNormalize = (form: Readonly<{ _tag: string }>, self: string): string => {
  const name = constructorName(form._tag);
  return isForm(name) ? self.normalize(name) : self;
};

export const padStart = (maxLength: number, fill: string) => (self: string) => self.padStart(maxLength, fill);
export const padEnd = (maxLength: number, fill: string) => (self: string) => self.padEnd(maxLength, fill);
export const repeat = (n: number) => (self: string) => self.repeat(count(n));

const CR = 13;
const LF = 10;

/** The lines split at `\n`, `\r` or `\r\n`, with their breaks when `keep`; no empty last line. */
const linesOf = (s: string, keep: boolean): Array<string> => {
  const out: Array<string> = [];
  let i = 0;
  while (i < s.length) {
    const start = i;
    while (i < s.length && s.charCodeAt(i) !== CR && s.charCodeAt(i) !== LF) i++;
    let end = i;
    if (i < s.length) {
      i += s.charCodeAt(i) === CR && s.charCodeAt(i + 1) === LF ? 2 : 1;
      if (keep) end = i;
    }
    out.push(s.substring(start, end));
  }
  return out;
};

export const flickLines = (self: string): Array<string> => linesOf(self, false);
export const flickLinesWithSeparators = (self: string): Array<string> => linesOf(self, true);

const stripLine = (line: string, margin: string): string => {
  let i = 0;
  while (i < line.length && line.charAt(i) <= " ") i++;
  return i < line.length && line.charAt(i) === margin ? line.substring(i + 1) : line;
};

export const stripMarginWith = (margin: string) => (self: string) =>
  linesOf(self, true)
    .map((line) => stripLine(line, margin))
    .join("");
export const stripMargin = (self: string): string => stripMarginWith("|")(self);

const isAllUnderscores = (s: string): boolean => s.length % 2 === 0 && s[0] === "_" && /^_+$/.test(s);

const snakeTo = (first: (c: string) => string) => (self: string) => {
  if (self.length === 0 || isAllUnderscores(self)) return "";
  let str = first(self.charAt(0));
  for (let i = 1; i < self.length; i++) {
    str += self[i] === "_" ? self.charAt(++i).toUpperCase() : self.charAt(i);
  }
  return str;
};

export const snakeToCamel = snakeTo((c) => c);
export const snakeToPascal = snakeTo((c) => c.toUpperCase());
export const snakeToKebab = (self: string): string => self.replace(/_/g, "-");
export const camelToSnake = (self: string): string => self.replace(/([A-Z])/g, "_$1").toLowerCase();
export const pascalToSnake = (self: string): string =>
  (self.slice(0, 1) + self.slice(1).replace(/([A-Z])/g, "_$1")).toLowerCase();
export const kebabToSnake = (self: string): string => self.replace(/-/g, "_");

const SPLIT = [/([a-z0-9])([A-Z])/g, /([A-Z])([A-Z][a-z])/g, /([A-Z])([0-9])/gi, /([0-9])([A-Z])/gi];
const CONFIG_SPLIT = [/([a-z0-9])([A-Z])/g, /([A-Z])([A-Z][a-z])/g];
const STRIP = /[^A-Z0-9]+/gi;

/** Effect's `normalizeCase`: the words, split and stripped, each transformed, joined by `delimiter`. */
const normalizeCase =
  (splitters: ReadonlyArray<RegExp>, delimiter: string, transform: (word: string, i: number) => string) =>
  (input: string): string => {
    let result = splitters.reduce((s, re) => s.replace(re, "$1\0$2"), input).replace(STRIP, "\0");
    let start = 0;
    let end = result.length;
    while (result.charAt(start) === "\0") start++;
    while (result.charAt(end - 1) === "\0") end--;
    if (start >= end) return "";
    result = result.slice(start, end);
    return result.split("\0").map(transform).join(delimiter);
  };

const lower = (word: string): string => word.toLowerCase();
const upper = (word: string): string => word.toUpperCase();
const pascalWord = (word: string): string => word.charAt(0).toUpperCase() + word.substring(1).toLowerCase();

export const noCase = normalizeCase(SPLIT, " ", lower);
export const pascalCase = normalizeCase(SPLIT, "", pascalWord);
export const camelCase = normalizeCase(SPLIT, "", (word, i) => (i === 0 ? lower(word) : pascalWord(word)));
export const constantCase = normalizeCase(SPLIT, "_", upper);
export const configCase = normalizeCase(CONFIG_SPLIT, "_", upper);
export const kebabCase = normalizeCase(SPLIT, "-", lower);
export const snakeCase = normalizeCase(SPLIT, "_", lower);
