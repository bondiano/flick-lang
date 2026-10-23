// Lists, non-empty lists, mutable JS arrays and chunks. A callback comes
// adapted to its JS arity, `f(a, b)`; a runtime callback returns an `Effect`.
import { Chunk, Effect, Option } from "effect";

import { EQ, LT, type Ordering, type Tagged, none, some } from "../data.ts";
import { item } from "../internal.ts";

type List<A> = ReadonlyArray<A>;

export const identity = <A>(x: A): A => x;

export const listMap = <A, B>(f: (a: A) => B, xs: List<A>): Array<B> => xs.map((x) => f(x));
export const listMapIndexed = <A, B>(f: (i: number, a: A) => B, xs: List<A>): Array<B> => xs.map((x, i) => f(i, x));
export const listMapEffect = <A, B, E, R>(f: (a: A) => Effect.Effect<B, E, R>, xs: List<A>): Effect.Effect<Array<B>, E, R> =>
  Effect.forEach(xs, (x) => f(x));
export const listEachEffect = <A, E, R>(f: (a: A) => Effect.Effect<unknown, E, R>, xs: List<A>): Effect.Effect<void, E, R> =>
  Effect.forEach(xs, (x) => f(x), { discard: true });

export const listFold = <A, B>(f: (acc: B, a: A) => B, init: B, xs: List<A>): B => {
  let acc = init;
  for (const x of xs) acc = f(acc, x);
  return acc;
};

export const listFoldRight = <A, B>(f: (a: A, acc: B) => B, init: B, xs: List<A>): B =>
  xs.reduceRight((acc, x) => f(x, acc), init);

export const listFoldEffect = <A, B, E, R>(
  f: (acc: B, a: A) => Effect.Effect<B, E, R>,
  init: B,
  xs: List<A>,
): Effect.Effect<B, E, R> => Effect.reduce(xs, () => init, (acc, x) => f(acc, x));

export const listFilter = <A>(p: (a: A) => boolean, xs: List<A>): Array<A> => xs.filter((x) => p(x));

export const listFilterMap = <A, B>(f: (a: A) => Option.Option<B>, xs: List<A>): Array<B> => {
  const out: Array<B> = [];
  for (const x of xs) {
    const o = f(x);
    if (Option.isSome(o)) out.push(o.value);
  }
  return out;
};

export const listFlatMap = <A, B>(f: (a: A) => List<B>, xs: List<A>): Array<B> => xs.flatMap((x) => f(x));

export const listFind = <A>(p: (a: A) => boolean, xs: List<A>): Option.Option<A> => {
  for (const x of xs) if (p(x)) return some(x);
  return none;
};

export const listFindIndex = <A>(p: (a: A) => boolean, xs: List<A>): Option.Option<number> => {
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

const order = (o: Ordering): number => (o._tag === LT._tag ? -1 : o._tag === EQ._tag ? 0 : 1);

/** A stable sort: `Array.prototype.sort` is stable, on a copy. */
export const listSortBy = <A>(cmp: (a: A, b: A) => Ordering, xs: List<A>): Array<A> =>
  xs.toSorted((a, b) => order(cmp(a, b)));

export const listLength = <A>(xs: List<A>): number => xs.length;

const index = (i: number, n: number): boolean => Number.isInteger(i) && i >= 0 && i < n;

export const listGet = <A>(i: number, xs: List<A>): Option.Option<A> => (index(i, xs.length) ? some(item(xs, i)) : none);

export const listSet = <A>(i: number, x: A, xs: List<A>): List<A> => (index(i, xs.length) ? xs.with(i, x) : xs);

export const listSingleton = <A>(x: A): Array<A> => [x];

const count = (n: number): number => Math.max(0, Math.trunc(n)) || 0;

export const listRepeat = <A>(n: number, x: A): Array<A> => Array.from({ length: count(n) }, () => x);

export const listRange = (from: number, until: number): Array<number> => {
  const out: Array<number> = [];
  for (let x = from; x < until; x++) out.push(x);
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

export const nonEmptyFromList = <A>(xs: List<A>): Option.Option<List<A>> => (xs.length > 0 ? some(xs) : none);
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

export const chunkEmpty: Chunk.Chunk<never> = Chunk.empty();
export const chunkFromList = <A>(xs: List<A>): Chunk.Chunk<A> => Chunk.fromIterable(xs);
export const chunkToList = <A>(c: Chunk.Chunk<A>): Array<A> => Chunk.toArray(c);
export const chunkAppend = <A>(x: A, c: Chunk.Chunk<A>): Chunk.Chunk<A> => Chunk.append(c, x);
export const chunkConcat = <A>(a: Chunk.Chunk<A>, b: Chunk.Chunk<A>): Chunk.Chunk<A> => Chunk.appendAll(a, b);
export const chunkSize = <A>(c: Chunk.Chunk<A>): number => Chunk.size(c);
export const chunkGet = <A>(i: number, c: Chunk.Chunk<A>): Option.Option<A> =>
  index(i, Chunk.size(c)) ? some(Chunk.getUnsafe(c, i)) : none;
export const chunkMap = <A, B>(f: (a: A) => B, c: Chunk.Chunk<A>): Chunk.Chunk<B> => Chunk.map(c, (x) => f(x));

/** `Monad (Eff r e)` and `bind_eff`. */
export const succeed = <A>(x: A): Effect.Effect<A> => Effect.succeed(x);
export const flatMapEff = <A, B, E, R, E2, R2>(
  f: (a: A) => Effect.Effect<B, E2, R2>,
  eff: Effect.Effect<A, E, R>,
): Effect.Effect<B, E | E2, R | R2> => Effect.flatMap(eff, (x) => f(x));
export const mapEff = <A, B, E, R>(f: (a: A) => B, eff: Effect.Effect<A, E, R>): Effect.Effect<B, E, R> =>
  Effect.map(eff, (x) => f(x));
export const bindEff = <A, B, E, R, E2, R2>(
  eff: Effect.Effect<A, E, R>,
  f: (a: A) => Effect.Effect<B, E2, R2>,
): Effect.Effect<B, E | E2, R | R2> => Effect.flatMap(eff, (x) => f(x));

const CONTINUE = "Loop.Step.Continue";

/** `Loop.Step s a`. */
export type Step<S, A> = Tagged<typeof CONTINUE, S> | Tagged<"Loop.Step.Done", A>;

/** `Loop.loop step state`: runs until `Done`, without growing the stack. */
export const loop = <S, A>(step: (s: S) => Step<S, A>, state: S): A => {
  let s = state;
  for (;;) {
    const next = step(s);
    if (next._tag !== CONTINUE) return next._0;
    s = next._0;
  }
};

/** `Trampoline a`. */
export type Trampoline<A> =
  | { readonly done: true; readonly value: A }
  | { readonly done: false; readonly thunk: (unit: undefined) => Trampoline<A> };

export const trampolineDone = <A>(value: A): Trampoline<A> => ({ done: true, value });
export const trampolineSuspend = <A>(thunk: (unit: undefined) => Trampoline<A>): Trampoline<A> => ({ done: false, thunk });
export const trampolineRun = <A>(t: Trampoline<A>): A => {
  let current = t;
  while (!current.done) current = current.thunk(undefined);
  return current.value;
};
