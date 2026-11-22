// Persistent collections retain their equality witness across the boundary.
import { Chunk } from "effect";

import type { Chunk as NativeChunk } from "@flick-lang/runtime/rt/chunk";
import * as Native from "@flick-lang/runtime/rt/collections";
import * as Host from "./stdlib/hash.ts";

type Map<K, V> = Native.FlickHashMap<K, V> | Host.FlickHashMap<K, V>;
type Set<A> = Native.FlickHashSet<A> | Host.FlickHashSet<A>;

export const hashMapToEffect = <K, V>(value: Map<K, V>): Host.FlickHashMap<K, V> =>
  "native" in value ? value : Host.hashMapFromSeq(value.witness, Native.hashMapToSeq(value));
export const hashMapFromEffect = <K, V>(value: Map<K, V>): Native.FlickHashMap<K, V> =>
  !("native" in value) ? value : Native.hashMapFromSeq(value.witness, Host.hashMapToSeq(value));

export const hashSetToEffect = <A>(value: Set<A>): Host.FlickHashSet<A> =>
  "native" in value ? value : Host.hashSetFromSeq(value.witness, Native.hashSetToSeq(value));
export const hashSetFromEffect = <A>(value: Set<A>): Native.FlickHashSet<A> =>
  !("native" in value) ? value : Native.hashSetFromSeq(value.witness, Host.hashSetToSeq(value));

type ChunkValue<A> = NativeChunk<A> | Chunk.Chunk<A>;
export const chunkToEffect = <A>(value: ChunkValue<A>): Chunk.Chunk<A> => Chunk.fromIterable(value);
export const chunkFromEffect = <A>(value: ChunkValue<A>): NativeChunk<A> =>
  Chunk.isChunk(value) ? Chunk.toReadonlyArray(value) : value;
