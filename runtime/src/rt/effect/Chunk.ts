// The pure operations of Flick's persistent chunks.
import { ChunkBranch } from "../../core/chunk.ts";
import { equals, LT, GT, none, some as optionSome, type Option, type Result, type Ordering } from "../../core/data.ts";
import { item } from "../../internal.ts";
import {
  chunkArray, chunkConcat, chunkItem, chunkMap, chunkReduce, chunkReduceRight, chunkSlice, chunkToList, type Chunk,
} from "../chunk.ts";
const count = (n: number): number => Number.isFinite(n) ? Math.max(0, Math.floor(n)) : 0;
export const flickNonEmpty = <A>(chunk: Chunk<A>): Option<Chunk<A>> => chunk.length ? optionSome(chunk) : none;
export const isEmpty = <A>(chunk: Chunk<A>): boolean => chunk.length === 0;
export const of = <A>(value: A): Array<A> => [value];
export const make = <A>(first: A, ...rest: ReadonlyArray<A>): Array<A> => [first, ...rest];
export const makeBy = <A>(n: number, f: (index: number) => A): Array<A> =>
  Array.from({ length: Math.max(1, count(n)) }, (_, index) => f(index));
export const range = (start: number, end: number): Array<number> =>
  Array.from({ length: Math.max(1, count(end - start + 1)) }, (_, index) => start + index);
export const head = <A>(chunk: Chunk<A>): Option<A> => chunk.length ? optionSome(chunkItem(chunk, 0)) : none;
export const headNonEmpty = <A>(chunk: Chunk<A>): A => chunkItem(chunk, 0);
export const last = <A>(chunk: Chunk<A>): Option<A> => chunk.length ? optionSome(chunkItem(chunk, chunk.length - 1)) : none;
export const lastNonEmpty = <A>(chunk: Chunk<A>): A => chunkItem(chunk, chunk.length - 1);
export const tail = <A>(chunk: Chunk<A>): Option<Chunk<A>> => chunk.length ? optionSome(chunkSlice(chunk, 1)) : none;
export const tailNonEmpty = <A>(chunk: Chunk<A>): Chunk<A> => chunkSlice(chunk, 1);
export const prepend = <A>(value: A) => (chunk: Chunk<A>): Chunk<A> => chunkConcat([value], chunk);
export const appendAll = <A>(that: Chunk<A>) => (self: Chunk<A>): Chunk<A> => chunkConcat(self, that);
export const take = (n: number) => <A>(chunk: Chunk<A>): Chunk<A> => chunkSlice(chunk, 0, count(n));
export const takeRight = (n: number) => <A>(chunk: Chunk<A>): Chunk<A> => chunkSlice(chunk, Math.max(0, chunk.length - count(n)));
export const drop = (n: number) => <A>(chunk: Chunk<A>): Chunk<A> => chunkSlice(chunk, count(n));
export const dropRight = (n: number) => <A>(chunk: Chunk<A>): Chunk<A> => chunkSlice(chunk, 0, Math.max(0, chunk.length - count(n)));
const findIndex = <A>(predicate: (value: A) => boolean, chunk: Chunk<A>): number => {
  let index = 0;
  for (const value of chunk) {
    if (predicate(value)) return index;
    index++;
  }
  return -1;
};
const prefix = <A>(predicate: (value: A) => boolean, chunk: Chunk<A>): number => {
  const index = findIndex((value) => !predicate(value), chunk);
  return index < 0 ? chunk.length : index;
};
export const takeWhile = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Chunk<A> => chunkSlice(chunk, 0, prefix(predicate, chunk));
export const dropWhile = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Chunk<A> => chunkSlice(chunk, prefix(predicate, chunk));
export const splitAt = (n: number) => <A>(chunk: Chunk<A>): [Chunk<A>, Chunk<A>] => {
  const index = count(n);
  return [chunkSlice(chunk, 0, index), chunkSlice(chunk, index)];
};
export const splitNonEmptyAt = (n: number) => <A>(chunk: Chunk<A>): [Chunk<A>, Chunk<A>] =>
  splitAt(Math.max(1, count(n)))(chunk);
export const chunksOf = (n: number) => <A>(chunk: Chunk<A>): Array<Array<A>> => {
  const width = Math.max(1, count(n));
  const values = chunkArray(chunk);
  const result: Array<Array<A>> = [];
  for (let index = 0; index < values.length; index += width) result.push(values.slice(index, index + width));
  return result;
};
export const split = (n: number) => <A>(chunk: Chunk<A>): Array<Array<A>> =>
  chunksOf(Math.ceil(chunk.length / Math.max(1, count(n))))(chunk);
export const splitWhere = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): [Chunk<A>, Chunk<A>] => {
  const index = findIndex(predicate, chunk);
  return index < 0 ? [chunk, []] : [chunkSlice(chunk, 0, index), chunkSlice(chunk, index)];
};
export const remove = (index: number) => <A>(chunk: Chunk<A>): Array<A> => {
  if (index < 0 || index >= chunk.length) return chunkToList(chunk);
  const values = chunkArray(chunk);
  return [...values.slice(0, index), ...values.slice(index + 1)];
};
export const modify = <A>(index: number, f: (value: A) => A) => (chunk: Chunk<A>): Option<Array<A>> => {
  if (index < 0 || index >= chunk.length) return none;
  const copy = chunkToList(chunk);
  copy[index] = f(item(copy, index));
  return optionSome(copy);
};
export const replace = <A>(index: number, value: A) => (chunk: Chunk<A>): Option<Array<A>> =>
  modify<A>(index, () => value)(chunk);
export const reverse = <A>(chunk: Chunk<A>): Array<A> => chunkArray(chunk).toReversed();
export const filter = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Array<A> => chunkArray(chunk).filter(predicate);
export const filterMap = <A, B, E>(f: (value: A) => Result<B, E>) => (chunk: Chunk<A>): Array<B> =>
  chunkArray(chunk).flatMap((value) => { const result = f(value); return result._tag === "Success" ? [result.success] : []; });
export const filterMapWhile = <A, B, E>(f: (value: A) => Result<B, E>) => (chunk: Chunk<A>): Array<B> => {
  const result: Array<B> = [];
  for (const value of chunk) { const next = f(value); if (next._tag === "Failure") break; result.push(next.success); }
  return result;
};
export const compact = <A>(chunk: Chunk<Option<A>>): Array<A> => chunkArray(chunk).flatMap((value) => value._tag === "Some" ? [value.value] : []);
export const flatMap = <A, B>(f: (value: A) => Chunk<B>) => (chunk: Chunk<A>): Array<B> => chunkArray(chunk).flatMap((value) => chunkToList(f(value)));
export const flatten = <A>(chunk: Chunk<Chunk<A>>): Array<A> => chunkArray(chunk).flatMap((value) => chunkToList(value));
export const containsWith = <A>(same: (a: A, b: A) => boolean) => (value: A) => (chunk: Chunk<A>): boolean =>
  some((candidate: A) => same(value, candidate))(chunk);
export const dedupe = <A>(chunk: Chunk<A>): Array<A> => {
  const result: Array<A> = [];
  for (const value of chunk) if (!result.some((other) => equals(value, other))) result.push(value);
  return result;
};
export const dedupeAdjacent = <A>(chunk: Chunk<A>): Array<A> =>
  chunkArray(chunk).filter((value, index, values) => index === 0 || !equals(item(values, index - 1), value));
export const differenceWith = <A>(same: (a: A, b: A) => boolean) => (that: Chunk<A>) => (self: Chunk<A>): Array<A> => {
  const others = chunkArray(that);
  return chunkArray(self).filter((value) => !others.some((other) => same(value, other)));
};
export const intersection = <A>(that: Chunk<A>) => (self: Chunk<A>): Array<A> => {
  const others = chunkArray(that);
  return dedupe(chunkArray(self).filter((value) => others.some((other) => equals(value, other))));
};
export const union = <A>(that: Chunk<A>) => (self: Chunk<A>): Array<A> => dedupe(chunkConcat(self, that));
export const every = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): boolean =>
  findIndex((value) => !predicate(value), chunk) < 0;
export const some = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): boolean =>
  findIndex(predicate, chunk) >= 0;
export const findFirst = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Option<A> => {
  const index = findIndex(predicate, chunk);
  return index < 0 ? none : optionSome(chunkItem(chunk, index));
};
export const findFirstIndex = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Option<number> => {
  const index = findIndex(predicate, chunk);
  return index < 0 ? none : optionSome(index);
};
const lastIndex = <A>(predicate: (value: A) => boolean, chunk: Chunk<A>): number => {
  if (!(chunk instanceof ChunkBranch)) return chunk.findLastIndex(predicate);
  const index = lastIndex(predicate, chunk.right);
  return index < 0 ? lastIndex(predicate, chunk.left) : chunk.leftLength + index;
};
export const findLast = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Option<A> => {
  const index = lastIndex(predicate, chunk);
  return index < 0 ? none : optionSome(chunkItem(chunk, index));
};
export const findLastIndex = <A>(predicate: (value: A) => boolean) => (chunk: Chunk<A>): Option<number> => {
  const index = lastIndex(predicate, chunk);
  return index < 0 ? none : optionSome(index | 0);
};
export const join = (separator: string) => (chunk: Chunk<string>): string => chunkArray(chunk).join(separator);
export const mapAccum = <S, A, B>(initial: S, f: (state: S, value: A) => readonly [S, B]) =>
  (chunk: Chunk<A>): readonly [S, Array<B>] => {
    let state = initial;
    const mapped: Array<B> = [];
    for (const value of chunk) { const next = f(state, value); state = next[0]; mapped.push(next[1]); }
    return [state, mapped];
  };
export const partition = <A, B, E>(f: (value: A) => Result<B, E>) => (chunk: Chunk<A>): [Array<B>, Array<E>] =>
  separate(chunkMap(f, chunk));
export const separate = <A, E>(chunk: Chunk<Result<A, E>>): [Array<A>, Array<E>] => {
  const successes: Array<A> = [];
  const failures: Array<E> = [];
  for (const result of chunk) { if (result._tag === "Success") successes.push(result.success); else failures.push(result.failure); }
  return [successes, failures];
};
export const reduce = <A, B>(initial: B, f: (acc: B, value: A) => B) => (chunk: Chunk<A>): B => chunkReduce(chunk, initial, f);
export const reduceRight = <A, B>(initial: B, f: (acc: B, value: A) => B) =>
  (chunk: Chunk<A>): B => chunkReduceRight(chunk, initial, f);
export const flickSortBy = <A>(compare: (a: A, b: A) => Ordering) => (chunk: Chunk<A>): Array<A> =>
  chunkArray(chunk).toSorted((a, b) => { const order = compare(a, b)._tag; return order === LT._tag ? -1 : order === GT._tag ? 1 : 0; });
export const unzip = <A, B>(chunk: Chunk<readonly [A, B]>): [Array<A>, Array<B>] => {
  const values = chunkArray(chunk);
  return [values.map(([a]) => a), values.map(([, b]) => b)];
};
export const zipWith = <A, B, C>(that: Chunk<B>, f: (a: A, b: B) => C) => (self: Chunk<A>): Array<C> => {
  const length = Math.min(self.length, that.length);
  const left = chunkArray(chunkSlice(self, 0, length));
  const right = chunkArray(chunkSlice(that, 0, length));
  return left.map((value, index) => f(value, item(right, index)));
};
export const zip = <B>(that: Chunk<B>) => <A>(self: Chunk<A>): Array<[A, B]> =>
  zipWith(that, (left: A, right: B): [A, B] => [left, right])(self);
