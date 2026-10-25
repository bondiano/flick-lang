import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

import * as CoreArray from "../src/core/Array.ts";
import * as CoreIterable from "../src/core/Iterable.ts";
import * as CoreString from "../src/core/String.ts";
import { EQ } from "../src/core/data.ts";
import * as EffectArray from "../src/effect/Array.ts";
import * as EffectIterable from "../src/effect/Iterable.ts";
import * as EffectString from "../src/effect/String.ts";
import { tag } from "../src/tags.ts";

// Both sides are called through `any`: the cases call them as compiled code does, not as their types say.
// oxlint-disable typescript/no-explicit-any, typescript/no-unsafe-call, typescript/no-unsafe-member-access, typescript/no-unsafe-return, typescript/no-unsafe-argument
type Impl = any;
type Case = (m: Impl) => unknown;

/** Effect's Option and Result as the plain data of `core`, recursively. */
const plain = (v: unknown): unknown => {
  if (Array.isArray(v)) return v.map(plain);
  if (typeof v !== "object" || v === null) return v;
  const field = (k: string): unknown => plain(Reflect.get(v, k));
  switch (Reflect.get(v, "_tag")) {
    case "None":
      return { _tag: "None" };
    case "Some":
      return { _tag: "Some", value: field("value") };
    case "Success":
      return { _tag: "Success", success: field("success") };
    case "Failure":
      return { _tag: "Failure", failure: field("failure") };
    default:
      return Object.fromEntries(Object.entries(v).map(([k, x]) => [k, plain(x)]));
  }
};

const ok = (success: unknown) => ({ _tag: "Success", success });
const err = (failure: unknown) => ({ _tag: "Failure", failure });
const some = (value: unknown) => ({ _tag: "Some", value });
const none = { _tag: "None" };

const each = <A>(xs: ReadonlyArray<A>, f: (x: A) => Case): Array<Case> => xs.map(f);

const texts = ["", "a", "abc", "  hi  ", "a\nb\r\nc\rd\n", "\n", "\r\n\r\n", "x\n", "😀é", "İstanbul", "ß"];
const words = [
  "",
  "fooBar",
  "FooBar",
  "foo_bar",
  "foo-bar baz",
  "XMLHttpRequest",
  "version2Api",
  "v2",
  "__",
  "_a_b",
  "a__b",
  "_",
  "___",
  "already_snake_",
  "  --  ",
  "ABC123def",
];
const indices = [0, 1, 2, -1, -3, 10, 1.7, -0.5, Number.NaN];
const counts = [0, 1, 2, 3, -1, 1.5, 100, Number.NaN, Infinity];
const finite = counts.filter(Number.isFinite);
const nfc = (name: string) => ({ _tag: tag(`String.NormalForm.${name}`) });

const stringCases: Record<string, Array<Case>> = {
  trim: each(texts, (s) => (m) => m.trim(s)),
  trimStart: each(texts, (s) => (m) => m.trimStart(s)),
  trimEnd: each(texts, (s) => (m) => m.trimEnd(s)),
  length: each(texts, (s) => (m) => m.length(s)),
  isNonEmpty: each(texts, (s) => (m) => m.isNonEmpty(s)),
  split: each(["", ",", "a", "b"], (sep) => (m) => texts.concat(["a,b,,c", "ba"]).map((s) => m.split(sep)(s))),
  includes: each(["", "a", "bc", "z"], (p) => (m) => texts.map((s) => m.includes(p)(s))),
  startsWith: each(["", "a", " "], (p) => (m) => texts.map((s) => m.startsWith(p)(s))),
  endsWith: each(["", "c", "\n"], (p) => (m) => texts.map((s) => m.endsWith(p)(s))),
  indexOf: each(["", "a", "b", "z"], (p) => (m) => texts.map((s) => m.indexOf(p)(s))),
  lastIndexOf: each(["", "a", "b", "z"], (p) => (m) => texts.map((s) => m.lastIndexOf(p)(s))),
  toUpperCase: each(texts, (s) => (m) => m.toUpperCase(s)),
  toLowerCase: each(texts, (s) => (m) => m.toLowerCase(s)),
  toLocaleUpperCase: each(["en", "tr"], (l) => (m) => texts.map((s) => m.toLocaleUpperCase(l)(s))),
  toLocaleLowerCase: each(["en", "tr"], (l) => (m) => texts.map((s) => m.toLocaleLowerCase(l)(s))),
  capitalize: each(texts.concat(["élan", "😀x"]), (s) => (m) => m.capitalize(s)),
  uncapitalize: each(texts.concat(["ÉLAN", "ABC"]), (s) => (m) => m.uncapitalize(s)),
  slice: each(indices, (a) => (m) => indices.map((b) => m.slice(a, b)("abcdef"))),
  substring: each(indices, (a) => (m) => indices.map((b) => m.substring(a, b)("abcdef"))),
  takeLeft: each(counts, (n) => (m) => texts.map((s) => m.takeLeft(n)(s))),
  takeRight: each(counts, (n) => (m) => texts.map((s) => m.takeRight(n)(s))),
  at: each(indices, (i) => (m) => texts.map((s) => m.at(i)(s))),
  charAt: each(indices, (i) => (m) => texts.map((s) => m.charAt(i)(s))),
  charCodeAt: each(indices, (i) => (m) => texts.map((s) => m.charCodeAt(i)(s))),
  codePointAt: each(indices, (i) => (m) => texts.map((s) => m.codePointAt(i)(s))),
  flickLocaleCompare: each(["en", "sv", "de"], (l) => (m) =>
    [["a", "b"], ["b", "a"], ["a", "a"], ["ä", "z"], ["", "a"]].map(([x, y]) => m.flickLocaleCompare(l, x, y)),
  ),
  flickNormalize: each(["NFC", "NFD", "NFKC", "NFKD", "Other"], (f) => (m) =>
    ["é", "é", "ﬁ", ""].map((s) => m.flickNormalize(nfc(f), s)),
  ),
  padStart: each(finite, (n) => (m) => ["", "ab"].map((s) => m.padStart(n, "xy")(s))),
  padEnd: each(finite, (n) => (m) => ["", "ab"].map((s) => m.padEnd(n, "xy")(s))),
  repeat: each(finite, (n) => (m) => texts.map((s) => m.repeat(n)(s))),
  flickLines: each(texts.concat(["\r", "a\r\n", "\n\n", "a\rb"]), (s) => (m) => m.flickLines(s)),
  flickLinesWithSeparators: each(texts.concat(["\r", "a\r\n", "\n\n"]), (s) => (m) => m.flickLinesWithSeparators(s)),
  stripMargin: each(["  |a\n  |b", "|x\r\n  y\n\t|z", "", "   ", "a|b"], (s) => (m) => m.stripMargin(s)),
  stripMarginWith: each(["#", "|"], (c) => (m) => ["  #a\n  |b", " #"].map((s) => m.stripMarginWith(c)(s))),
  snakeToCamel: each(words, (s) => (m) => m.snakeToCamel(s)),
  snakeToPascal: each(words, (s) => (m) => m.snakeToPascal(s)),
  snakeToKebab: each(words, (s) => (m) => m.snakeToKebab(s)),
  camelToSnake: each(words, (s) => (m) => m.camelToSnake(s)),
  pascalToSnake: each(words, (s) => (m) => m.pascalToSnake(s)),
  kebabToSnake: each(words, (s) => (m) => m.kebabToSnake(s)),
  noCase: each(words, (s) => (m) => m.noCase(s)),
  pascalCase: each(words, (s) => (m) => m.pascalCase(s)),
  camelCase: each(words, (s) => (m) => m.camelCase(s)),
  constantCase: each(words, (s) => (m) => m.constantCase(s)),
  configCase: each(words, (s) => (m) => m.configCase(s)),
  kebabCase: each(words, (s) => (m) => m.kebabCase(s)),
  snakeCase: each(words, (s) => (m) => m.snakeCase(s)),
};

const lists: ReadonlyArray<ReadonlyArray<number>> = [[], [1], [1, 2], [3, 1, 2, 3, 1], [1, 1, 2, 2, 2, 1], [5, 4, 3, 2, 1, 0]];
const odd = (x: number) => x % 2 === 1;
const small = (x: number) => x < 3;
const mod3 = (a: number, b: number) => a % 3 === b % 3;
const close = (a: number, b: number) => Math.abs(a - b) <= 1;
const half = (x: number) => (x % 2 === 0 ? ok(x / 2) : err(`odd ${x}`));
const over = (f: (xs: ReadonlyArray<number>) => unknown) => lists.map(f);
const listEach = (f: (m: Impl, xs: ReadonlyArray<number>) => unknown): Array<Case> => [(m) => over((xs) => f(m, xs))];
const results = [ok(1), err("a"), ok(2), err("b")];
const cases = { onEmpty: (u: undefined) => ["empty", u], onNonEmpty: (xs: unknown) => ["non-empty", xs] };

const arrayCases: Record<string, Array<Case>> = {
  findLast: [(m) => over(m.findLast(odd)), (m) => over(m.findLast(small))],
  findLastIndex: [(m) => over(m.findLastIndex(odd)), (m) => over(m.findLastIndex(small))],
  findFirstWithIndex: [(m) => over(m.findFirstWithIndex(odd)), (m) => over(m.findFirstWithIndex(small))],
  countBy: [(m) => over(m.countBy(odd))],
  containsWith: each([0, 1, 4, 9], (x) => (m) => over(m.containsWith(mod3)(x))),
  dedupeWith: [(m) => over(m.dedupeWith(mod3)), (m) => over(m.dedupeWith(close))],
  dedupeAdjacentWith: [(m) => over(m.dedupeAdjacentWith(close)), (m) => over(m.dedupeAdjacentWith(mod3))],
  differenceWith: lists.map((ys) => (m) => over(m.differenceWith(close)(ys))),
  intersectionWith: lists.map((ys) => (m) => over(m.intersectionWith(close)(ys))),
  unionWith: lists.map((ys) => (m) => over(m.unionWith(ys, close))),
  groupBy: [
    (m) => over(m.groupBy((x: number) => (odd(x) ? "odd" : "even"))),
    (m) => over(m.groupBy((x: number) => (x === 1 ? "__proto__" : String(x)))),
  ],
  join: each(["", ", "], (sep) => (m) => [[], ["a"], ["a", "b", "c"]].map((xs) => m.join(sep)(xs))),
  takeRight: counts.map((n) => (m) => over(m.takeRight(n))),
  dropRight: counts.map((n) => (m) => over(m.dropRight(n))),
  takeWhile: [(m) => over(m.takeWhile(odd)), (m) => over(m.takeWhile(small))],
  dropWhile: [(m) => over(m.dropWhile(odd)), (m) => over(m.dropWhile(small))],
  takeWhileFilter: [(m) => over(m.takeWhileFilter(half)), (m) => over(m.takeWhileFilter((x: number) => ok(x)))],
  dropWhileFilter: [(m) => over(m.dropWhileFilter(half)), (m) => over(m.dropWhileFilter((x: number) => ok(x)))],
  span: [(m) => over(m.span(odd)), (m) => over(m.span(small))],
  splitAt: counts.map((n) => (m) => over(m.splitAt(n))),
  split: counts.map((n) => (m) => over(m.split(n))),
  splitWhere: [(m) => over(m.splitWhere(odd)), (m) => over(m.splitWhere(small))],
  chunksOf: counts.map((n) => (m) => over(m.chunksOf(n))),
  window: counts.map((n) => (m) => over(m.window(n))),
  chop: [(m) => over(m.chop((xs: ReadonlyArray<number>) => [xs.length, xs.slice(2)]))],
  tail: [(m) => over(m.tail)],
  init: [(m) => over(m.init)],
  pad: finite.map((n) => (m) => over(m.pad(n, 9))),
  rotate: [-7, -5, -2, -1, -0.4, 0, 1, 1.5, 2, 5, 6, 12, Number.NaN, Infinity].map((n) => (m) => over(m.rotate(n))),
  insertAt: indices.concat([5, 6]).map((i) => (m) => over(m.insertAt(i, 9))),
  remove: indices.concat([5, 6]).map((i) => (m) => over(m.remove(i))),
  modify: [-1, 0, 1, 1.9, 5, 6].map((i) => (m) => over(m.modify(i, (x: number) => x * 10))),
  replace: [-1, 0, 1, 1.9, 5, 6].map((i) => (m) => over(m.replace(i, 9))),
  intersperse: [(m) => over(m.intersperse(0))],
  appendAll: lists.map((ys) => (m) => over(m.appendAll(ys))),
  cartesian: lists.map((ys) => (m) => over(m.cartesian(ys))),
  cartesianWith: lists.map((ys) => (m) => over(m.cartesianWith(ys, (a: number, b: number) => a * 10 + b))),
  zipWith: lists.map((ys) => (m) => over(m.zipWith(ys, (a: number, b: number) => [a, b]))),
  mapAccum: [(m) => over(m.mapAccum(0, (s: number, a: number) => [s + a, s * a]))],
  scan: [(m) => over(m.scan(0, (b: number, a: number) => b * 2 + a))],
  scanRight: [(m) => over(m.scanRight(0, (b: number, a: number) => b * 2 + a))],
  extend: [(m) => over(m.extend((xs: ReadonlyArray<number>) => xs.length))],
  getSomes: [(m) => m.getSomes([]), (m) => m.getSomes([some(1), none, some(2), none])],
  getFailures: [(m) => m.getFailures([]), (m) => m.getFailures(results)],
  getSuccesses: [(m) => m.getSuccesses([]), (m) => m.getSuccesses(results)],
  separate: [(m) => m.separate([]), (m) => m.separate(results)],
  partition: [(m) => over(m.partition(half))],
  unfold: [
    (m) => m.unfold(1, (n: number) => (n > 100 ? none : some([n, n * 3]))),
    (m) => m.unfold(0, () => none),
  ],
  liftOption: [(m) => [0, 1, 2].map(m.liftOption((x: number) => (x > 0 ? some(x) : none)))],
  liftPredicate: [(m) => [0, 1, 2].map(m.liftPredicate(odd))],
  liftResult: [(m) => [0, 1, 2].map(m.liftResult(half))],
  match: listEach((m, xs) => m.match(cases)(xs)),
  flickMatchLeft: listEach((m, xs) =>
    m.flickMatchLeft({ onEmpty: cases.onEmpty, onNonEmpty: (h: number) => (t: unknown) => [h, t] })(xs),
  ),
  flickMatchRight: listEach((m, xs) =>
    m.flickMatchRight({ onEmpty: cases.onEmpty, onNonEmpty: (i: unknown) => (l: number) => [i, l] })(xs),
  ),
};

const iterableCases: Record<string, Array<Case>> = {
  flickFilterMapWhile: [(m) => over(m.flickFilterMapWhile(half)), (m) => over(m.flickFilterMapWhile(ok))],
  flickRepeat: finite.map((n) => (m) => over(m.flickRepeat(n))),
};

const builtins = new URL("../../flick-core/src/types/builtins/", import.meta.url);

/** The JS names the `.fl` files bind from `@flick-lang/runtime/effect/<module>`. */
const bound = (files: ReadonlyArray<string>, module: string): Array<string> => {
  const re = new RegExp(`foreign \\w+ \\w+ from "@flick-lang/runtime/effect/${module}" "(\\w+)"`, "g");
  const names = files.flatMap((f) => [...readFileSync(new URL(f, builtins), "utf8").matchAll(re)].map((m) => m[1] ?? ""));
  return [...new Set(names)].toSorted();
};

const groups = [
  { module: "String", files: ["String.fl"], core: CoreString, effect: EffectString, cases: stringCases },
  {
    module: "Array",
    files: ["List/Search.fl", "List/Slice.fl", "List/Transform.fl"],
    core: CoreArray,
    effect: EffectArray,
    cases: arrayCases,
  },
  { module: "Iterable", files: ["List/Transform.fl"], core: CoreIterable, effect: EffectIterable, cases: iterableCases },
];

for (const g of groups) {
  test(`core/${g.module} exports exactly the names the .fl files bind`, () => {
    const names = bound(g.files, g.module);
    assert.deepEqual(Object.keys(g.core).toSorted(), names);
    assert.deepEqual(Object.keys(g.cases).toSorted(), names);
  });

  for (const [name, fns] of Object.entries(g.cases)) {
    test(`core/${g.module}.${name} matches effect`, () => {
      for (const [i, run] of fns.entries()) {
        assert.deepEqual(plain(run(g.core)), plain(run(g.effect)), `${name} case ${i}`);
      }
    });
  }
}

test("core/String.flickLocaleCompare gives Prelude orderings", () => {
  assert.equal(CoreString.flickLocaleCompare("en", "a", "a"), EQ);
});
