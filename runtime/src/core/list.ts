// Lists, non-empty lists and mutable JS arrays, without Effect. A callback
// comes adapted to its JS arity, `f(a, b)`.
import { EQ, LT, type Option, type Ordering, type Result, err, none, ok, some } from "./data.ts";
import { item } from "../internal.ts";

type List<A> = ReadonlyArray<A>;

export const identity = <A>(x: A): A => x;

export const listMap = <A, B>(f: (a: A) => B, xs: List<A>): Array<B> => xs.map((x) => f(x));
export const listMapIndexed = <A, B>(f: (i: number, a: A) => B, xs: List<A>): Array<B> => xs.map((x, i) => f(i, x));
export const listFold = <A, B>(f: (acc: B, a: A) => B, init: B, xs: List<A>): B => {
  let acc = init;
  for (const x of xs) acc = f(acc, x);
  return acc;
};

export const listFoldRight = <A, B>(f: (a: A, acc: B) => B, init: B, xs: List<A>): B =>
  xs.reduceRight((acc, x) => f(x, acc), init);

export const listFilter = <A>(p: (a: A) => boolean, xs: List<A>): Array<A> => xs.filter((x) => p(x));

export const listFilterMap = <A, B>(f: (a: A) => Option<B>, xs: List<A>): Array<B> => {
  const out: Array<B> = [];
  for (const x of xs) {
    const o = f(x);
    if (o._tag === "Some") out.push(o.value);
  }
  return out;
};

export const listFlatMap = <A, B>(f: (a: A) => List<B>, xs: List<A>): Array<B> => xs.flatMap((x) => f(x));

/** `Applicative.map2` of lists: every pair, the first list outermost. */
export const listMap2 = <A, B, C>(f: (a: A, b: B) => C, xs: List<A>, ys: List<B>): Array<C> =>
  xs.flatMap((x) => ys.map((y) => f(x, y)));

export const optionMap2 = <A, B, C>(f: (a: A, b: B) => C, x: Option<A>, y: Option<B>): Option<C> =>
  x._tag === "Some" && y._tag === "Some" ? some(f(x.value, y.value)) : none;

export const resultMap2 = <A, B, C, E>(f: (a: A, b: B) => C, x: Result<A, E>, y: Result<B, E>): Result<C, E> =>
  x._tag === "Failure" ? x : y._tag === "Failure" ? y : ok(f(x.success, y.success));

/** `map2` of a `Validation`: the errors of both, combined, or `f` of both values. */
export const validationMap2 = <A, B, C, E>(
  combine: (a: E, b: E) => E,
  f: (a: A, b: B) => C,
  x: Result<A, E>,
  y: Result<B, E>,
): Result<C, E> => {
  if (x._tag === "Failure") return y._tag === "Failure" ? err(combine(x.failure, y.failure)) : x;
  return y._tag === "Failure" ? y : ok(f(x.success, y.success));
};

export const listFind = <A>(p: (a: A) => boolean, xs: List<A>): Option<A> => {
  for (const x of xs) if (p(x)) return some(x);
  return none;
};

export const listFindIndex = <A>(p: (a: A) => boolean, xs: List<A>): Option<number> => {
  const i = xs.findIndex((x) => p(x));
  return i < 0 ? none : some(i);
};

export const listAny = <A>(p: (a: A) => boolean, xs: List<A>): boolean => xs.some((x) => p(x));
export const listAll = <A>(p: (a: A) => boolean, xs: List<A>): boolean => xs.every((x) => p(x));

export const listPartition = <A>(p: (a: A) => boolean, xs: List<A>): [Array<A>, Array<A>] => {
  const yes: Array<A> = [];
  const no: Array<A> = [];
  for (const x of xs) (p(x) ? yes : no).push(x);
  return [yes, no];
};

const order = (o: Ordering): -1 | 0 | 1 => (o._tag === LT._tag ? -1 : o._tag === EQ._tag ? 0 : 1);

/** An Effect `Order` from a Flick comparison, for the bindings of `effect/*`. */
export const flickOrder =
  <A>(cmp: (a: A, b: A) => Ordering) =>
  (a: A, b: A): -1 | 0 | 1 =>
    order(cmp(a, b));

/** A stable sort: `Array.prototype.sort` is stable, on a copy. */
export const listSortBy = <A>(cmp: (a: A, b: A) => Ordering, xs: List<A>): Array<A> =>
  xs.toSorted((a, b) => order(cmp(a, b)));

export const listLength = <A>(xs: List<A>): number => xs.length;

const index = (i: number, n: number): boolean => Number.isInteger(i) && i >= 0 && i < n;

export const listGet = <A>(i: number, xs: List<A>): Option<A> => (index(i, xs.length) ? some(item(xs, i)) : none);

export const listSet = <A>(i: number, x: A, xs: List<A>): List<A> => (index(i, xs.length) ? xs.with(i, x) : xs);

export const listSingleton = <A>(x: A): Array<A> => [x];

const count = (n: number): number => Math.max(0, Math.trunc(n)) || 0;

export const listRepeat = <A>(n: number, x: A): Array<A> => Array.from({ length: count(n) }, () => x);

export const listRange = (from: number, until: number): Array<number> => {
  const out: Array<number> = [];
  for (let x = from; x < until; x++) {
    out.push(x);
    // past 2^53 the step may not change `x`
    if (x + 1 === x) break;
  }
  return out;
};

export const listInitialize = <A>(n: number, f: (i: number) => A): Array<A> => Array.from({ length: count(n) }, (_, i) => f(i));

export const listReverse = <A>(xs: List<A>): Array<A> => xs.toReversed();
export const listConcat = <A>(xss: List<List<A>>): Array<A> => xss.flat(1);
export const listTake = <A>(n: number, xs: List<A>): Array<A> => xs.slice(0, Math.max(0, n));
export const listDrop = <A>(n: number, xs: List<A>): Array<A> => xs.slice(Math.max(0, n));
export const listSlice = <A>(start: number, end: number, xs: List<A>): Array<A> => xs.slice(start, end);

export const listZip = <A, B>(xs: List<A>, ys: List<B>): Array<[A, B]> => {
  const n = Math.min(xs.length, ys.length);
  const out: Array<[A, B]> = [];
  for (let i = 0; i < n; i++) out.push([item(xs, i), item(ys, i)]);
  return out;
};

export const listUnzip = <A, B>(pairs: List<readonly [A, B]>): [Array<A>, Array<B>] => [
  pairs.map((p) => p[0]),
  pairs.map((p) => p[1]),
];

export const nonEmptyFromList = <A>(xs: List<A>): Option<List<A>> => (xs.length > 0 ? some(xs) : none);
export const nonEmptyCons = <A>(x: A, xs: List<A>): Array<A> => [x, ...xs];
export const nonEmptyHead = <A>(xs: readonly [A, ...Array<A>]): A => xs[0];

export const jsArrayMake = <A>(): Array<A> => [];
export const jsArrayFromList = <A>(xs: List<A>): Array<A> => [...xs];
export const jsArrayPush = <A>(array: Array<A>, x: A): void => {
  array.push(x);
};
export const jsArraySet = <A>(i: number, x: A, array: Array<A>): void => {
  if (index(i, array.length + 1)) array[i] = x;
};
