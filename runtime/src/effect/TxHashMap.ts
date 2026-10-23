// Effect's `TxHashMap` for the bindings of the standard library. A Flick
// `TxHashMap w k v` keeps the witness of its keys beside Effect's map, whose
// keys are wrapped with it as in `HashMap w k v`, so each operation is an
// adapter that wraps keys going in and unwraps them coming out.
import { Effect, Option, type Result, TxHashMap } from "effect";

import { none, some } from "../data.ts";
import { type FlickHashMap, Key, type Witness } from "../stdlib/hash.ts";

export * from "effect/TxHashMap";

/** `TxHashMap w k v`. */
export interface FlickTxHashMap<K, V> {
  readonly witness: Witness<K>;
  readonly native: TxHashMap.TxHashMap<Key<K>, V>;
}

type Entry<K, V> = readonly [K, V];

const wrap = <K, V>(witness: Witness<K>, native: TxHashMap.TxHashMap<Key<K>, V>): FlickTxHashMap<K, V> =>
  Object.freeze({ witness, native });

const key = <K, V>(self: FlickTxHashMap<K, V>, k: K): Key<K> => new Key(k, self.witness);

const keyed = <K, V>(witness: Witness<K>, entries: ReadonlyArray<Entry<K, V>>): Array<Entry<Key<K>, V>> =>
  entries.map(([k, v]) => [new Key(k, witness), v] as const);

const same =
  <K, V>(self: FlickTxHashMap<K, V>) =>
  <B>(effect: Effect.Effect<TxHashMap.TxHashMap<Key<K>, B>>): Effect.Effect<FlickTxHashMap<K, B>> =>
    Effect.map(effect, (native) => wrap(self.witness, native));

export const flickEmpty = <K, V>(witness: Witness<K>): Effect.Effect<FlickTxHashMap<K, V>> =>
  Effect.map(TxHashMap.empty<Key<K>, V>(), (native) => wrap(witness, native));

/** `TxHashMap.make`: the entries as Effect's rest arguments. */
export const flickMake = <K, V>(
  witness: Witness<K>,
  entries: ReadonlyArray<Entry<K, V>>,
): Effect.Effect<FlickTxHashMap<K, V>> =>
  Effect.map(TxHashMap.make(...keyed(witness, entries)), (native) => wrap(witness, native));

export const flickFromIterable = <K, V>(
  witness: Witness<K>,
  entries: ReadonlyArray<Entry<K, V>>,
): Effect.Effect<FlickTxHashMap<K, V>> =>
  Effect.map(TxHashMap.fromIterable(keyed(witness, entries)), (native) => wrap(witness, native));

export const flickGet = <K, V>(k: K, self: FlickTxHashMap<K, V>): Effect.Effect<Option.Option<V>> =>
  TxHashMap.get(self.native, key(self, k));
export const flickSet = <K, V>(k: K, v: V, self: FlickTxHashMap<K, V>): Effect.Effect<void> =>
  TxHashMap.set(self.native, key(self, k), v);
export const flickHas = <K, V>(k: K, self: FlickTxHashMap<K, V>): Effect.Effect<boolean> =>
  TxHashMap.has(self.native, key(self, k));
export const flickRemove = <K, V>(k: K, self: FlickTxHashMap<K, V>): Effect.Effect<boolean> =>
  TxHashMap.remove(self.native, key(self, k));
export const flickClear = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<void> => TxHashMap.clear(self.native);
export const flickSize = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<number> => TxHashMap.size(self.native);
export const flickIsEmpty = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<boolean> =>
  TxHashMap.isEmpty(self.native);
export const flickIsNonEmpty = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<boolean> =>
  TxHashMap.isNonEmpty(self.native);
export const flickModify = <K, V>(k: K, f: (v: V) => V, self: FlickTxHashMap<K, V>): Effect.Effect<Option.Option<V>> =>
  TxHashMap.modify(self.native, key(self, k), f);
export const flickModifyAt = <K, V>(
  k: K,
  f: (v: Option.Option<V>) => Option.Option<V>,
  self: FlickTxHashMap<K, V>,
): Effect.Effect<void> => TxHashMap.modifyAt(self.native, key(self, k), f);

export const flickKeys = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<Array<K>> =>
  Effect.map(TxHashMap.keys(self.native), (keys) => keys.map((k) => k.value));
export const flickValues = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<Array<V>> =>
  TxHashMap.values(self.native);
export const flickEntries = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<Array<Entry<K, V>>> =>
  Effect.map(TxHashMap.entries(self.native), (entries) => entries.map(([k, v]) => [k.value, v] as const));
export const flickToEntries = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<Array<Entry<K, V>>> =>
  Effect.map(TxHashMap.toEntries(self.native), (entries) => entries.map(([k, v]) => [k.value, v] as const));
export const flickToValues = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<Array<V>> =>
  TxHashMap.toValues(self.native);

/** `TxHashMap.snapshot`: a `HashMap w k v` of the same witness. */
export const flickSnapshot = <K, V>(self: FlickTxHashMap<K, V>): Effect.Effect<FlickHashMap<K, V>> =>
  Effect.map(TxHashMap.snapshot(self.native), (native) => Object.freeze({ witness: self.witness, native }));
export const flickUnion = <K, V>(other: FlickHashMap<K, V>, self: FlickTxHashMap<K, V>): Effect.Effect<void> =>
  TxHashMap.union(self.native, other.native);
export const flickRemoveMany = <K, V>(keys: ReadonlyArray<K>, self: FlickTxHashMap<K, V>): Effect.Effect<void> =>
  TxHashMap.removeMany(
    self.native,
    keys.map((k) => key(self, k)),
  );
export const flickSetMany = <K, V>(entries: ReadonlyArray<Entry<K, V>>, self: FlickTxHashMap<K, V>): Effect.Effect<void> =>
  TxHashMap.setMany(self.native, keyed(self.witness, entries));
export const flickGetHash = <K, V>(k: K, hash: number, self: FlickTxHashMap<K, V>): Effect.Effect<Option.Option<V>> =>
  TxHashMap.getHash(self.native, key(self, k), hash);
export const flickHasHash = <K, V>(k: K, hash: number, self: FlickTxHashMap<K, V>): Effect.Effect<boolean> =>
  TxHashMap.hasHash(self.native, key(self, k), hash);

export const flickMap = <K, V, B>(f: (v: V, k: K) => B, self: FlickTxHashMap<K, V>): Effect.Effect<FlickTxHashMap<K, B>> =>
  same(self)(TxHashMap.map(self.native, (v, k) => f(v, k.value)));
export const flickFilter = <K, V>(
  f: (v: V, k: K) => boolean,
  self: FlickTxHashMap<K, V>,
): Effect.Effect<FlickTxHashMap<K, V>> => same(self)(TxHashMap.filter(self.native, (v, k) => f(v, k.value)));
export const flickFilterMap = <K, V, B, X>(
  f: (v: V, k: K) => Result.Result<B, X>,
  self: FlickTxHashMap<K, V>,
): Effect.Effect<FlickTxHashMap<K, B>> => same(self)(TxHashMap.filterMap(self.native, (v, k) => f(v, k.value)));
export const flickCompact = <K, V>(self: FlickTxHashMap<K, Option.Option<V>>): Effect.Effect<FlickTxHashMap<K, V>> =>
  same(self)(TxHashMap.compact(self.native));
export const flickFlatMap = <K, V, B>(
  f: (v: V, k: K) => Effect.Effect<FlickTxHashMap<K, B>>,
  self: FlickTxHashMap<K, V>,
): Effect.Effect<FlickTxHashMap<K, B>> =>
  same(self)(
    TxHashMap.flatMap(self.native, (v, k) =>
      Effect.map(f(v, k.value), (m) => m.native),
    ),
  );

export const flickReduce = <K, V, A>(
  zero: A,
  f: (acc: A, v: V, k: K) => A,
  self: FlickTxHashMap<K, V>,
): Effect.Effect<A> => TxHashMap.reduce(self.native, zero, (acc, v, k) => f(acc, v, k.value));
export const flickHasBy = <K, V>(f: (v: V, k: K) => boolean, self: FlickTxHashMap<K, V>): Effect.Effect<boolean> =>
  TxHashMap.hasBy(self.native, (v, k) => f(v, k.value));
export const flickSome = <K, V>(f: (v: V, k: K) => boolean, self: FlickTxHashMap<K, V>): Effect.Effect<boolean> =>
  TxHashMap.some(self.native, (v, k) => f(v, k.value));
export const flickEvery = <K, V>(f: (v: V, k: K) => boolean, self: FlickTxHashMap<K, V>): Effect.Effect<boolean> =>
  TxHashMap.every(self.native, (v, k) => f(v, k.value));
export const flickFindFirst = <K, V>(
  f: (v: V, k: K) => boolean,
  self: FlickTxHashMap<K, V>,
): Effect.Effect<Option.Option<Entry<K, V>>> =>
  Effect.map(TxHashMap.findFirst(self.native, (v, k) => f(v, k.value)), (found) =>
    Option.isSome(found) ? some([found.value[0].value, found.value[1]] as const) : none,
  );
export const flickForEach = <K, V, E, R>(
  f: (v: V, k: K) => Effect.Effect<void, E, R>,
  self: FlickTxHashMap<K, V>,
): Effect.Effect<void, E, R> => TxHashMap.forEach(self.native, (v, k) => f(v, k.value));

