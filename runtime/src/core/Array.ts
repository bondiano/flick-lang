// The `Array` bindings of `List` without Effect: the semantics of
// `effect/Array` (4.0.5) on plain data, curried as the bindings call them.
// A callback of the program takes one argument, or two uncurried; the index
// Effect passes besides is never read by it, so it is not passed.
import { item } from "../internal.ts";
import { none, type Option, type Result, some } from "./data.ts";

type List<A> = ReadonlyArray<A>;
type Eqv<A> = (a: A, b: A) => boolean;

/** Effect's `Count.normalize`: a whole count, 0 for a negative one or NaN. */
const count = (n: number): number => (n > 0 ? Math.floor(n) : 0);

const isOutOfBounds = (i: number, xs: List<unknown>): boolean => !Number.isFinite(i) || i < 0 || i >= xs.length;

export const findLast =
  <A>(p: (a: A) => boolean) =>
  (self: List<A>): Option<A> => {
    for (let i = self.length - 1; i >= 0; i--) {
      const a = item(self, i);
      if (p(a)) return some(a);
    }
    return none;
  };

export const findLastIndex =
  <A>(p: (a: A) => boolean) =>
  (self: List<A>): Option<number> => {
    const i = self.findLastIndex((a) => p(a));
    return i >= 0 ? some(i) : none;
  };

export const findFirstWithIndex =
  <A>(p: (a: A) => boolean) =>
  (self: List<A>): Option<[A, number]> => {
    const i = self.findIndex((a) => p(a));
    return i >= 0 ? some([item(self, i), i]) : none;
  };

export const countBy =
  <A>(p: (a: A) => boolean) =>
  (self: List<A>): number =>
    self.reduce((n, a) => (p(a) ? n + 1 : n), 0);

/** `eq` gets `a` first, then the element. */
export const containsWith =
  <A>(eq: Eqv<A>) =>
  (a: A) =>
  (self: List<A>): boolean =>
    self.some((x) => eq(a, x));

/** `eq` gets the candidate first, then a kept element. */
const dedupe = <A>(xs: List<A>, eq: Eqv<A>): Array<A> => {
  const out: Array<A> = [];
  for (const x of xs) if (!out.some((kept) => eq(x, kept))) out.push(x);
  return out;
};

export const dedupeWith =
  <A>(eq: Eqv<A>) =>
  (self: List<A>): Array<A> =>
    dedupe(self, eq);

/** Compares with the last element kept, not the one before. */
export const dedupeAdjacentWith =
  <A>(eq: Eqv<A>) =>
  (self: List<A>): Array<A> => {
    const out: Array<A> = [];
    for (const a of self) if (out.length === 0 || !eq(a, item(out, out.length - 1))) out.push(a);
    return out;
  };

export const differenceWith =
  <A>(eq: Eqv<A>) =>
  (that: List<A>) =>
  (self: List<A>): Array<A> =>
    self.filter((a) => !that.some((b) => eq(a, b)));

export const intersectionWith =
  <A>(eq: Eqv<A>) =>
  (that: List<A>) =>
  (self: List<A>): Array<A> =>
    self.filter((a) => that.some((b) => eq(a, b)));

/** Returns `self` or `that` itself when the other is empty, as Effect does. */
export const unionWith =
  <A>(that: List<A>, eq: Eqv<A>) =>
  (self: List<A>): List<A> =>
    self.length === 0 || that.length === 0
      ? self.length === 0
        ? that
        : self
      : self.concat(that.filter((b) => !self.some((a) => eq(a, b))));

export const groupBy =
  <A>(f: (a: A) => string) =>
  (self: List<A>): Record<string, Array<A>> => {
    const out: Record<string, Array<A>> = {};
    for (const a of self) {
      const k = f(a);
      const group = Object.hasOwn(out, k) ? out[k] : undefined;
      if (group) group.push(a);
      // defineProperty: a `__proto__` key is a group, not the prototype
      else Object.defineProperty(out, k, { value: [a], writable: true, enumerable: true, configurable: true });
    }
    return out;
  };

export const join = (separator: string) => (self: List<string>) => self.join(separator);

export const takeRight =
  (n: number) =>
  <A>(self: List<A>): Array<A> => {
    const i = Math.min(count(n), self.length);
    return i === 0 ? [] : self.slice(-i);
  };

export const dropRight =
  (n: number) =>
  <A>(self: List<A>): Array<A> =>
    self.slice(0, self.length - Math.min(count(n), self.length));

const spanIndex = <A>(self: List<A>, p: (a: A) => boolean): number => {
  const i = self.findIndex((a) => !p(a));
  return i < 0 ? self.length : i;
};

export const takeWhile =
  <A>(p: (a: A) => boolean) =>
  (self: List<A>): Array<A> =>
    self.slice(0, spanIndex(self, p));

export const dropWhile =
  <A>(p: (a: A) => boolean) =>
  (self: List<A>): Array<A> =>
    self.slice(spanIndex(self, p));

export const takeWhileFilter =
  <A, B, E>(f: (a: A) => Result<B, E>) =>
  (self: List<A>): Array<B> => {
    const out: Array<B> = [];
    for (const a of self) {
      const r = f(a);
      if (r._tag === "Failure") break;
      out.push(r.success);
    }
    return out;
  };

export const dropWhileFilter =
  <A, B, E>(f: (a: A) => Result<B, E>) =>
  (self: List<A>): Array<A> =>
    self.slice(spanIndex(self, (a) => f(a)._tag === "Success"));

/** Effect's `splitAt` on a count already normalized. */
const cut = <A>(self: List<A>, n: number): [Array<A>, Array<A>] => [self.slice(0, n), self.slice(n)];

export const span =
  <A>(p: (a: A) => boolean) =>
  (self: List<A>): [Array<A>, Array<A>] =>
    cut(self, spanIndex(self, p));

export const splitAt =
  (n: number) =>
  <A>(self: List<A>): [Array<A>, Array<A>] =>
    cut(self, count(n));

export const splitWhere =
  <A>(p: (a: A) => boolean) =>
  (self: List<A>): [Array<A>, Array<A>] =>
    cut(self, spanIndex(self, (a) => !p(a)));

export const chunksOf =
  (n: number) =>
  <A>(self: List<A>): Array<Array<A>> => {
    const size = Math.max(1, count(n));
    const out: Array<Array<A>> = [];
    for (let i = 0; i < self.length; i += size) out.push(self.slice(i, i + size));
    return out;
  };

export const split =
  (n: number) =>
  <A>(self: List<A>): Array<Array<A>> =>
    chunksOf(Math.ceil(self.length / Math.max(1, count(n))))(self);

export const window =
  (n: number) =>
  <A>(self: List<A>): Array<Array<A>> => {
    const size = count(n);
    if (size === 0 || size > self.length) return [];
    return Array.from({ length: self.length - size + 1 }, (_, i) => self.slice(i, i + size));
  };

export const chop =
  <A, B>(f: (self: List<A>) => readonly [B, List<A>]) =>
  (self: List<A>): Array<B> => {
    const out: Array<B> = [];
    let next = self;
    while (next.length > 0) {
      const [b, rest] = f(next);
      out.push(b);
      next = rest;
    }
    return out;
  };

export const tail = <A>(self: List<A>): Option<Array<A>> => (self.length > 0 ? some(self.slice(1)) : none);
export const init = <A>(self: List<A>): Option<Array<A>> => (self.length > 0 ? some(self.slice(0, -1)) : none);

export const pad =
  <A>(n: number, fill: A) =>
  (self: List<A>): Array<A> => {
    const length = count(n);
    return self.length >= length ? self.slice(0, length) : self.concat(Array<A>(length - self.length).fill(fill));
  };

/** Rotated `round(n)` places to the right; a non-finite `n` does not rotate. */
export const rotate =
  (n: number) =>
  <A>(self: List<A>): Array<A> => {
    const m = Math.round(n) % self.length;
    if (self.length === 0 || !Number.isFinite(m) || m === 0) return self.slice();
    const at = m < 0 ? -m : self.length - m;
    return self.slice(at).concat(self.slice(0, at));
  };

export const insertAt =
  <A>(i: number, b: A) =>
  (self: List<A>): Option<Array<A>> => {
    const index = Math.floor(i);
    if (index !== self.length && isOutOfBounds(index, self)) return none;
    const out = self.slice();
    out.splice(index, 0, b);
    return some(out);
  };

export const remove =
  (i: number) =>
  <A>(self: List<A>): Array<A> => {
    const index = Math.floor(i);
    const out = self.slice();
    if (!isOutOfBounds(index, out)) out.splice(index, 1);
    return out;
  };

export const modify =
  <A>(i: number, f: (a: A) => A) =>
  (self: List<A>): Option<Array<A>> => {
    const index = Math.floor(i);
    if (isOutOfBounds(index, self)) return none;
    const out = self.slice();
    out[index] = f(item(self, index));
    return some(out);
  };

export const replace = <A>(i: number, b: A) => modify<A>(i, () => b);

export const intersperse =
  <A>(middle: A) =>
  (self: List<A>): Array<A> => {
    const out: Array<A> = [];
    for (const a of self) {
      if (out.length > 0) out.push(middle);
      out.push(a);
    }
    return out;
  };

export const appendAll =
  <A>(that: List<A>) =>
  (self: List<A>): Array<A> =>
    self.concat(that);

export const cartesianWith =
  <A, B, C>(that: List<B>, f: (a: A, b: B) => C) =>
  (self: List<A>): Array<C> =>
    self.flatMap((a) => that.map((b) => f(a, b)));

export const cartesian =
  <B>(that: List<B>) =>
  <A>(self: List<A>): Array<[A, B]> =>
    cartesianWith(that, (a: A, b: B): [A, B] => [a, b])(self);

export const zipWith =
  <A, B, C>(that: List<B>, f: (a: A, b: B) => C) =>
  (self: List<A>): Array<C> =>
    Array.from({ length: Math.min(self.length, that.length) }, (_, i) => f(item(self, i), item(that, i)));

export const mapAccum =
  <S, A, B>(s: S, f: (s: S, a: A) => readonly [S, B]) =>
  (self: List<A>): [S, Array<B>] => {
    let acc = s;
    const out = self.map((a) => {
      const [next, b] = f(acc, a);
      acc = next;
      return b;
    });
    return [acc, out];
  };

export const scan =
  <A, B>(b: B, f: (b: B, a: A) => B) =>
  (self: List<A>): Array<B> => {
    const out = [b];
    let acc = b;
    for (const a of self) out.push((acc = f(acc, a)));
    return out;
  };

export const scanRight =
  <A, B>(b: B, f: (b: B, a: A) => B) =>
  (self: List<A>): Array<B> => {
    const out = Array<B>(self.length + 1);
    let acc = b;
    out[self.length] = acc;
    for (let i = self.length - 1; i >= 0; i--) out[i] = acc = f(acc, item(self, i));
    return out;
  };

export const extend =
  <A, B>(f: (suffix: Array<A>) => B) =>
  (self: List<A>): Array<B> =>
    self.map((_, i) => f(self.slice(i)));

export const getSomes = <A>(self: List<Option<A>>): Array<A> =>
  self.flatMap((o) => (o._tag === "Some" ? [o.value] : []));

export const getFailures = <A, E>(self: List<Result<A, E>>): Array<E> =>
  self.flatMap((r) => (r._tag === "Failure" ? [r.failure] : []));

export const getSuccesses = <A, E>(self: List<Result<A, E>>): Array<A> =>
  self.flatMap((r) => (r._tag === "Success" ? [r.success] : []));

export const partition =
  <A, B, E>(f: (a: A) => Result<B, E>) =>
  (self: List<A>): [Array<B>, Array<E>] => {
    const passes: Array<B> = [];
    const fails: Array<E> = [];
    for (const a of self) {
      const r = f(a);
      if (r._tag === "Success") passes.push(r.success);
      else fails.push(r.failure);
    }
    return [passes, fails];
  };

export const separate = <A, E>(self: List<Result<A, E>>): [Array<A>, Array<E>] =>
  partition((r: Result<A, E>) => r)(self);

export const unfold = <A, B>(seed: B, f: (b: B) => Option<readonly [A, B]>): Array<A> => {
  const out: Array<A> = [];
  for (let o = f(seed); o._tag === "Some"; o = f(o.value[1])) out.push(o.value[0]);
  return out;
};

export const liftOption =
  <A, B>(f: (a: A) => Option<B>) =>
  (a: A): Array<B> => {
    const o = f(a);
    return o._tag === "Some" ? [o.value] : [];
  };

export const liftPredicate =
  <A>(p: (a: A) => boolean) =>
  (a: A): Array<A> =>
    p(a) ? [a] : [];

export const liftResult =
  <A, B, E>(f: (a: A) => Result<B, E>) =>
  (a: A): Array<B> => {
    const r = f(a);
    return r._tag === "Success" ? [r.success] : [];
  };

type Cases<B, F> = { readonly onEmpty: (u: undefined) => B; readonly onNonEmpty: F };

export const match =
  <A, B>(options: Cases<B, (self: List<A>) => B>) =>
  (self: List<A>): B =>
    self.length > 0 ? options.onNonEmpty(self) : options.onEmpty(undefined);

/** `List.match_left`: `onNonEmpty` is a curried Flick function of the head and the tail. */
export const flickMatchLeft =
  <A, B>(options: Cases<B, (head: A) => (tail: Array<A>) => B>) =>
  (self: List<A>): B =>
    self.length > 0 ? options.onNonEmpty(item(self, 0))(self.slice(1)) : options.onEmpty(undefined);

/** `List.match_right`: `onNonEmpty` is a curried Flick function of the init and the last. */
export const flickMatchRight =
  <A, B>(options: Cases<B, (init: Array<A>) => (last: A) => B>) =>
  (self: List<A>): B =>
    self.length > 0
      ? options.onNonEmpty(self.slice(0, -1))(item(self, self.length - 1))
      : options.onEmpty(undefined);
