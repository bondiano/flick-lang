// Persistent collections retain their equality witness across the boundary.
import { Chunk } from "effect";

import * as Native from "@flick-lang/runtime/rt/collections";
import * as Host from "./stdlib/hash.ts";

type Map<K, V> = Native.FlickHashMap<K, V> | Host.FlickHashMap<K, V>;
type Set<A> = Native.FlickHashSet<A> | Host.FlickHashSet<A>;

export const hashMapToEffect = <K, V>(value: Map<K, V>): Host.FlickHashMap<K, V> =>
  "native" in value ? value : Native.hashMapToList(value).reduce(
    (map, [key, item]) => Host.hashMapInsert(key, item, map), Host.hashMapWith<K, V>(value.witness));
export const hashMapFromEffect = <K, V>(value: Map<K, V>): Native.FlickHashMap<K, V> =>
  "entries" in value ? value : Native.hashMapFromList(value.witness, Host.hashMapToList(value));

export const hashSetToEffect = <A>(value: Set<A>): Host.FlickHashSet<A> =>
  "native" in value ? value : Host.hashSetFromList(value.witness, Native.hashSetToList(value));
export const hashSetFromEffect = <A>(value: Set<A>): Native.FlickHashSet<A> =>
  "map" in value ? value : Native.hashSetFromList(value.witness, Host.hashSetToList(value));

type ChunkValue<A> = ReadonlyArray<A> | Chunk.Chunk<A>;
export const chunkToEffect = <A>(value: ChunkValue<A>): Chunk.Chunk<A> => Chunk.fromIterable(value);
export const chunkFromEffect = <A>(value: ChunkValue<A>): ReadonlyArray<A> =>
  Chunk.isChunk(value) ? Chunk.toReadonlyArray(value) : value;
