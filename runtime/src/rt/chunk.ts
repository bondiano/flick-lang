import { ChunkBranch, concatChunks, type Chunk } from "../core/chunk.ts";
import { none, some, type Option } from "../core/data.ts";
import { item } from "../internal.ts";

export type { Chunk } from "../core/chunk.ts";
export const chunkEmpty: ReadonlyArray<never> = Object.freeze([]);
export const chunkFromList = <A>(values: ReadonlyArray<A>): Chunk<A> => [...values];
export const chunkAppend = <A>(value: A, chunk: Chunk<A>): Chunk<A> => concatChunks(chunk, [value]);
export const chunkConcat = concatChunks;
// Flick Int uses 32 bits even when structural sharing permits a larger length.
export const chunkSize = (chunk: Chunk<unknown>): number => chunk.length | 0;

const isFlat = <A>(chunk: Chunk<A>): chunk is ReadonlyArray<A> => Array.isArray(chunk);

export const chunkItem = <A>(chunk: Chunk<A>, index: number): A => {
  while (!isFlat(chunk)) {
    const leftSize = chunk.leftLength;
    if (index < leftSize) chunk = chunk.left;
    else {
      index -= leftSize;
      chunk = chunk.right;
    }
  }
  return item(chunk, index);
};

export const chunkGet = <A>(index: number, chunk: Chunk<A>): Option<A> =>
  Number.isInteger(index) && index >= 0 && index < chunk.length ? some(chunkItem(chunk, index)) : none;

/** Fold leaves directly to avoid materializing a temporary List. */
export const chunkReduce = <A, B>(chunk: Chunk<A>, initial: B, f: (acc: B, value: A) => B): B => {
  if (!isFlat(chunk)) return chunkReduce(chunk.right, chunkReduce(chunk.left, initial, f), f);
  let state = initial;
  for (let index = 0; index < chunk.length; index++) state = f(state, item(chunk, index));
  return state;
};

export const chunkReduceRight = <A, B>(chunk: Chunk<A>, initial: B, f: (acc: B, value: A) => B): B => {
  if (!isFlat(chunk)) return chunkReduceRight(chunk.left, chunkReduceRight(chunk.right, initial, f), f);
  let state = initial;
  for (let index = chunk.length - 1; index >= 0; index--) state = f(state, item(chunk, index));
  return state;
};

const copyTo = <A>(chunk: Chunk<A>, values: Array<A>, offset: number): void => {
  if (!isFlat(chunk)) {
    copyTo(chunk.left, values, offset);
    copyTo(chunk.right, values, offset + chunk.leftLength);
    return;
  }
  for (let index = 0; index < chunk.length; index++) values[offset + index] = item(chunk, index);
};

export const chunkToList = <A>(chunk: Chunk<A>): Array<A> => {
  if (isFlat(chunk)) return [...chunk];
  const values: Array<A> = [];
  values.length = chunk.length;
  copyTo(chunk, values, 0);
  return values;
};

/** The flat representation, for operations that do not mutate their input. */
export const chunkArray = <A>(chunk: Chunk<A>): ReadonlyArray<A> =>
  chunk instanceof ChunkBranch ? chunkToList(chunk) : chunk;

export const chunkSlice = <A>(chunk: Chunk<A>, start: number, end = chunk.length): Chunk<A> => {
  if (start >= end || start >= chunk.length) return chunkEmpty;
  if (start === 0 && end >= chunk.length) return chunk;
  if (!(chunk instanceof ChunkBranch)) return chunk.slice(start, end);
  const leftSize = chunk.leftLength;
  if (end <= leftSize) return chunkSlice(chunk.left, start, end);
  if (start >= leftSize) return chunkSlice(chunk.right, start - leftSize, end - leftSize);
  return concatChunks(chunkSlice(chunk.left, start), chunkSlice(chunk.right, 0, end - leftSize));
};

export const chunkMap = <A, B>(f: (value: A) => B, chunk: Chunk<A>): Array<B> => {
  if (!(chunk instanceof ChunkBranch)) return chunk.map(f);
  const values: Array<B> = [];
  values.length = chunk.length;
  chunkReduce(chunk, 0, (index, value) => {
    values[index] = f(value);
    return index + 1;
  });
  return values;
};
