// Chunk's core representation is a persistent dense array on the own runtime.
import { none, some, type Option } from "../core/data.ts";
import { item } from "../internal.ts";
export const chunkEmpty: ReadonlyArray<never> = Object.freeze([]);
export const chunkFromList = <A>(values: ReadonlyArray<A>): ReadonlyArray<A> => [...values];
export const chunkToList = <A>(values: ReadonlyArray<A>): Array<A> => [...values];
export const chunkAppend = <A>(value: A, chunk: ReadonlyArray<A>): Array<A> => [...chunk, value];
export const chunkConcat = <A>(first: ReadonlyArray<A>, second: ReadonlyArray<A>): Array<A> => [...first, ...second];
export const chunkSize = (chunk: ReadonlyArray<unknown>): number => chunk.length;
export const chunkGet = <A>(index: number, chunk: ReadonlyArray<A>): Option<A> =>
  Number.isInteger(index) && index >= 0 && index < chunk.length ? some(item(chunk, index)) : none;
export const chunkMap = <A, B>(f: (value: A) => B, chunk: ReadonlyArray<A>): Array<B> => chunk.map(f);
