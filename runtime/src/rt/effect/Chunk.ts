// The pure collection operations of `Chunk` over Flick's persistent array chunks.
import { equals, LT, GT, none, some as optionSome, type Option, type Result, type Ordering } from "../../core/data.ts";
import { item } from "../../internal.ts";
type Chunk<A> = ReadonlyArray<A>;
const count = (n: number): number => Number.isFinite(n) ? Math.max(0, Math.floor(n)) : 0;
export const flickNonEmpty = <A>(chunk: Chunk<A>): Option<Chunk<A>> => chunk.length ? optionSome(chunk) : none;
export const isEmpty = <A>(chunk: Chunk<A>): boolean => chunk.length === 0;
export const of = <A>(value: A): Array<A> => [value];
export const make = <A>(first: A, ...rest: ReadonlyArray<A>): Array<A> => [first, ...rest];
export const makeBy = <A>(n: number, f: (index: number) => A): Array<A> =>
  Array.from({ length: Math.max(1, count(n)) }, (_, index) => f(index));
export const range = (start: number, end: number): Array<number> =>
  Array.from({ length: Math.max(1, count(end - start + 1)) }, (_, index) => start + index);
export const head = <A>(chunk: Chunk<A>): Option<A> => chunk.length ? optionSome(item(chunk, 0)) : none;
export const headNonEmpty = <A>(chunk: Chunk<A>): A => item(chunk, 0);
export const last = <A>(chunk: Chunk<A>): Option<A> => chunk.length ? optionSome(item(chunk, chunk.length - 1)) : none;
export const lastNonEmpty = <A>(chunk: Chunk<A>): A => item(chunk, chunk.length - 1);
export const tail = <A>(chunk: Chunk<A>): Option<Array<A>> => chunk.length ? optionSome(chunk.slice(1)) : none;
export const tailNonEmpty = <A>(chunk: Chunk<A>): Array<A> => chunk.slice(1);
export const prepend = <A>(value: A) => (chunk: Chunk<A>): Array<A> => [value, ...chunk];
export const appendAll = <A>(that: Chunk<A>) => (self: Chunk<A>): Array<A> => [...self, ...that];
export const take = (n: number) => <A>(chunk: Chunk<A>): Array<A> => chunk.slice(0, count(n));
export const takeRight = (n: number) => <A>(chunk: Chunk<A>): Array<A> => chunk.slice(Math.max(0, chunk.length - count(n)));
export const drop = (n: number) => <A>(chunk: Chunk<A>): Array<A> => chunk.slice(count(n));
export const dropRight = (n: number) => <A>(chunk: Chunk<A>): Array<A> => chunk.slice(0, Math.max(0, chunk.length - count(n)));
const prefix = <A>(predicate: (value: A) => boolean, chunk: Chunk<A>): number => {
  const index = chunk.findIndex((value) => !predicate(value));
  return index < 0 ? chunk.length : index;
};
export const takeWhile = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Array<A> => chunk.slice(0, prefix(predicate, chunk));
export const dropWhile = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Array<A> => chunk.slice(prefix(predicate, chunk));
export const splitAt = (n: number) => <A>(chunk: Chunk<A>): [Array<A>, Array<A>] =>
  [chunk.slice(0, count(n)), chunk.slice(count(n))];
export const splitNonEmptyAt = (n: number) => <A>(chunk: Chunk<A>): [Array<A>, Array<A>] =>
  [chunk.slice(0, Math.max(1, count(n))), chunk.slice(Math.max(1, count(n)))];
export const chunksOf = (n: number) => <A>(chunk: Chunk<A>): Array<Array<A>> => {
  const width = Math.max(1, count(n));
  const result: Array<Array<A>> = [];
  for (let index = 0; index < chunk.length; index += width) result.push(chunk.slice(index, index + width));
  return result;
};
export const split = (n: number) => <A>(chunk: Chunk<A>): Array<Array<A>> =>
  chunksOf(Math.ceil(chunk.length / Math.max(1, count(n))))(chunk);
export const splitWhere = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): [Array<A>, Array<A>] => {
  const index = chunk.findIndex(predicate);
  return index < 0 ? [[...chunk], []] : [chunk.slice(0, index), chunk.slice(index)];
};
export const remove = (index: number) => <A>(chunk: Chunk<A>): Array<A> =>
  index < 0 || index >= chunk.length ? [...chunk] : [...chunk.slice(0, index), ...chunk.slice(index + 1)];
export const modify = <A>(index: number, f: (value: A) => A) => (chunk: Chunk<A>): Option<Array<A>> => {
  if (index < 0 || index >= chunk.length) return none;
  const copy = [...chunk];
  copy[index] = f(item(copy, index));
  return optionSome(copy);
};
export const replace = <A>(index: number, value: A) => (chunk: Chunk<A>): Option<Array<A>> =>
  modify<A>(index, () => value)(chunk);
export const reverse = <A>(chunk: Chunk<A>): Array<A> => chunk.toReversed();
export const filter = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Array<A> => chunk.filter(predicate);
export const filterMap = <A, B, E>(f: (value: A) => Result<B, E>) => (chunk: Chunk<A>): Array<B> =>
  chunk.flatMap((value) => { const result = f(value); return result._tag === "Success" ? [result.success] : []; });
export const filterMapWhile = <A, B, E>(f: (value: A) => Result<B, E>) => (chunk: Chunk<A>): Array<B> => {
  const result: Array<B> = [];
  for (const value of chunk) { const next = f(value); if (next._tag === "Failure") break; result.push(next.success); }
  return result;
};
export const compact = <A>(chunk: Chunk<Option<A>>): Array<A> => chunk.flatMap((value) => value._tag === "Some" ? [value.value] : []);
export const flatMap = <A, B>(f: (value: A) => Chunk<B>) => (chunk: Chunk<A>): Array<B> => chunk.flatMap((value) => [...f(value)]);
export const flatten = <A>(chunk: Chunk<Chunk<A>>): Array<A> => chunk.flatMap((value) => [...value]);
export const containsWith = <A>(same: (a: A, b: A) => boolean) => (value: A) => (chunk: Chunk<A>): boolean =>
  chunk.some((candidate) => same(value, candidate));
export const dedupe = <A>(chunk: Chunk<A>): Array<A> => {
  const result: Array<A> = [];
  for (const value of chunk) if (!result.some((other) => equals(value, other))) result.push(value);
  return result;
};
export const dedupeAdjacent = <A>(chunk: Chunk<A>): Array<A> =>
  chunk.filter((value, index) => index === 0 || !equals(item(chunk, index - 1), value));
export const differenceWith = <A>(same: (a: A, b: A) => boolean) => (that: Chunk<A>) => (self: Chunk<A>): Array<A> =>
  self.filter((value) => !that.some((other) => same(value, other)));
export const intersection = <A>(that: Chunk<A>) => (self: Chunk<A>): Array<A> =>
  dedupe(self.filter((value) => that.some((other) => equals(value, other))));
export const union = <A>(that: Chunk<A>) => (self: Chunk<A>): Array<A> => dedupe([...self, ...that]);
export const every = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): boolean => chunk.every(predicate);
export const some = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): boolean => chunk.some(predicate);
export const findFirst = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Option<A> => {
  const index = chunk.findIndex(predicate);
  return index < 0 ? none : optionSome(item(chunk, index));
};
export const findFirstIndex = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Option<number> => {
  const index = chunk.findIndex(predicate);
  return index < 0 ? none : optionSome(index);
};
export const findLast = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Option<A> => {
  const index = chunk.findLastIndex(predicate);
  return index < 0 ? none : optionSome(item(chunk, index));
};
export const findLastIndex = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Option<number> => {
  const index = chunk.findLastIndex(predicate);
  return index < 0 ? none : optionSome(index);
};
export const join = (separator: string) => (chunk: Chunk<string>): string => chunk.join(separator);
export const mapAccum = <S, A, B>(initial: S, f: (state: S, value: A) => readonly [S, B]) =>
  (chunk: Chunk<A>): readonly [S, Array<B>] => {
    let state = initial;
    const mapped: Array<B> = [];
    for (const value of chunk) { const next = f(state, value); state = next[0]; mapped.push(next[1]); }
    return [state, mapped];
  };
export const partition = <A, B, E>(f: (value: A) => Result<B, E>) => (chunk: Chunk<A>): [Array<B>, Array<E>] =>
  separate(chunk.map(f));
export const separate = <A, E>(chunk: Chunk<Result<A, E>>): [Array<A>, Array<E>] => {
  const successes: Array<A> = [];
  const failures: Array<E> = [];
  for (const result of chunk) { if (result._tag === "Success") successes.push(result.success); else failures.push(result.failure); }
  return [successes, failures];
};
export const reduce = <A, B>(initial: B, f: (acc: B, value: A) => B) => (chunk: Chunk<A>): B => chunk.reduce(f, initial);
export const reduceRight = <A, B>(initial: B, f: (acc: B, value: A) => B) =>
  (chunk: Chunk<A>): B => chunk.reduceRight((acc, value) => f(acc, value), initial);
export const flickSortBy = <A>(compare: (a: A, b: A) => Ordering) => (chunk: Chunk<A>): Array<A> =>
  chunk.toSorted((a, b) => { const order = compare(a, b)._tag; return order === LT._tag ? -1 : order === GT._tag ? 1 : 0; });
export const unzip = <A, B>(chunk: Chunk<readonly [A, B]>): [Array<A>, Array<B>] =>
  [chunk.map(([a]) => a), chunk.map(([, b]) => b)];
export const zip = <B>(that: Chunk<B>) => <A>(self: Chunk<A>): Array<[A, B]> =>
  self.slice(0, that.length).map((value, index) => [value, item(that, index)]);
export const zipWith = <A, B, C>(that: Chunk<B>, f: (a: A, b: B) => C) => (self: Chunk<A>): Array<C> =>
  self.slice(0, that.length).map((value, index) => f(value, item(that, index)));
