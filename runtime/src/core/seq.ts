// Lazy sequences, without Effect. A `Seq` is an iterable that starts over on
// every iteration, so it stays a value; a list is one already. A callback
// comes adapted to its JS arity, `f(a, b)`.
import { type Option, none, some } from "./data.ts";

export type Seq<A> = Iterable<A>;

/** A sequence whose every iteration runs `gen` afresh. */
export const seq = <A>(gen: () => Iterator<A>): Seq<A> => ({ [Symbol.iterator]: gen });

export const seqOfList = <A>(xs: ReadonlyArray<A>): Seq<A> => xs;

// Past 2^53 a step may not change a number: a range ends there instead
// of yielding it again forever.

export const seqFrom = (n: number): Seq<number> =>
  seq(function* () {
    for (let i = n; ; ) {
      yield i;
      if (i + 1 === i) return;
      i += 1;
    }
  });

export const seqRangeBy = (start: number, end: number, step: number): Seq<number> =>
  seq(function* () {
    if (!(step > 0 || step < 0)) return;
    for (let i = start; step > 0 ? i < end : i > end; ) {
      yield i;
      if (i + step === i) return;
      i += step;
    }
  });

export const seqRange = (start: number, end: number): Seq<number> => seqRangeBy(start, end, 1);

export const seqRangeTo = (start: number, end: number): Seq<number> =>
  seq(function* () {
    for (let i = start; i <= end; ) {
      yield i;
      if (i === end || i + 1 === i) return;
      i += 1;
    }
  });

export const seqIterate = <A>(f: (a: A) => A, x: A): Seq<A> =>
  seq(function* () {
    for (let v = x; ; v = f(v)) yield v;
  });

export const seqRepeat = <A>(x: A): Seq<A> =>
  seq(function* () {
    for (;;) yield x;
  });

export const seqCycle = <A>(xs: ReadonlyArray<A>): Seq<A> =>
  seq(function* () {
    if (xs.length > 0) for (;;) yield* xs;
  });

export const seqUnfold = <A, S>(f: (s: S) => Option<readonly [A, S]>, s: S): Seq<A> =>
  seq(function* () {
    for (let o = f(s); o._tag === "Some"; o = f(o.value[1])) yield o.value[0];
  });

export const seqAppend = <A>(xs: Seq<A>, ys: Seq<A>): Seq<A> =>
  seq(function* () {
    yield* xs;
    yield* ys;
  });

/** `Applicative.map2` of sequences: every pair, the first sequence outermost. */
export const seqMap2 = <A, B, C>(f: (a: A, b: B) => C, xs: Seq<A>, ys: Seq<B>): Seq<C> =>
  seq(function* () {
    for (const x of xs) for (const y of ys) yield f(x, y);
  });

export const seqMap = <A, B>(f: (a: A) => B, xs: Seq<A>): Seq<B> =>
  seq(function* () {
    for (const x of xs) yield f(x);
  });

export const seqFilter = <A>(p: (a: A) => boolean, xs: Seq<A>): Seq<A> =>
  seq(function* () {
    for (const x of xs) if (p(x)) yield x;
  });

export const seqFilterMap = <A, B>(f: (a: A) => Option<B>, xs: Seq<A>): Seq<B> =>
  seq(function* () {
    for (const x of xs) {
      const o = f(x);
      if (o._tag === "Some") yield o.value;
    }
  });

export const seqFlatMap = <A, B>(f: (a: A) => Seq<B>, xs: Seq<A>): Seq<B> =>
  seq(function* () {
    for (const x of xs) yield* f(x);
  });

/** At most `n` elements, `n` truncated as by `List.take`, pulling none after the last. */
export const seqTake = <A>(n: number, xs: Seq<A>): Seq<A> =>
  seq(function* () {
    let left = Math.trunc(n);
    if (!(left > 0)) return;
    for (const x of xs) {
      yield x;
      if (--left === 0) return;
    }
  });

export const seqTakeWhile = <A>(p: (a: A) => boolean, xs: Seq<A>): Seq<A> =>
  seq(function* () {
    for (const x of xs) {
      if (!p(x)) return;
      yield x;
    }
  });

export const seqDrop = <A>(n: number, xs: Seq<A>): Seq<A> =>
  seq(function* () {
    let skip = Math.trunc(n);
    for (const x of xs) {
      if (skip > 0) skip--;
      else yield x;
    }
  });

export const seqDropWhile = <A>(p: (a: A) => boolean, xs: Seq<A>): Seq<A> =>
  seq(function* () {
    let dropping = true;
    for (const x of xs) {
      if (dropping && p(x)) continue;
      dropping = false;
      yield x;
    }
  });

export const seqZipWith = <A, B, C>(f: (a: A, b: B) => C, xs: Seq<A>, ys: Seq<B>): Seq<C> =>
  seq(function* () {
    const other = ys[Symbol.iterator]();
    for (const x of xs) {
      const y = other.next();
      if (y.done) return;
      yield f(x, y.value);
    }
  });

export const seqZip = <A, B>(xs: Seq<A>, ys: Seq<B>): Seq<readonly [A, B]> => seqZipWith((a, b) => [a, b] as const, xs, ys);

export const seqEnumerate = <A>(xs: Seq<A>): Seq<readonly [number, A]> => seqZip(seqFrom(0), xs);

/** `init`, then each running fold of the elements. */
export const seqScan = <A, B>(f: (acc: B, a: A) => B, init: B, xs: Seq<A>): Seq<B> =>
  seq(function* () {
    let acc = init;
    yield acc;
    for (const x of xs) yield (acc = f(acc, x));
  });

/** Lists of `n` elements, the last one shorter when the elements run out. */
export const seqChunks = <A>(n: number, xs: Seq<A>): Seq<Array<A>> =>
  seq(function* () {
    if (n <= 0) return;
    let chunk: Array<A> = [];
    for (const x of xs) {
      chunk.push(x);
      if (chunk.length === n) {
        yield chunk;
        chunk = [];
      }
    }
    if (chunk.length > 0) yield chunk;
  });

/** Every run of `n` neighbouring elements, from the first. */
export const seqWindows = <A>(n: number, xs: Seq<A>): Seq<Array<A>> =>
  seq(function* () {
    if (n <= 0) return;
    const window: Array<A> = [];
    for (const x of xs) {
      window.push(x);
      if (window.length > n) window.shift();
      if (window.length === n) yield window.slice();
    }
  });

export const seqToList = <A>(xs: Seq<A>): Array<A> => Array.from(xs);

export const seqFold = <A, B>(f: (acc: B, a: A) => B, init: B, xs: Seq<A>): B => {
  let acc = init;
  for (const x of xs) acc = f(acc, x);
  return acc;
};

export const seqAny = <A>(p: (a: A) => boolean, xs: Seq<A>): boolean => {
  for (const x of xs) if (p(x)) return true;
  return false;
};

export const seqAll = <A>(p: (a: A) => boolean, xs: Seq<A>): boolean => {
  for (const x of xs) if (!p(x)) return false;
  return true;
};

export const seqFind = <A>(p: (a: A) => boolean, xs: Seq<A>): Option<A> => {
  for (const x of xs) if (p(x)) return some(x);
  return none;
};

export const seqFirst = <A>(xs: Seq<A>): Option<A> => {
  for (const x of xs) return some(x);
  return none;
};
