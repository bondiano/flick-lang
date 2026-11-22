// Effect's `HashMap` for the bindings of the standard library, and the
// adapters over Flick's maps: each key is boxed with the map's witness, so
// callbacks get the key itself and results keep the witness.
import { HashMap, Option } from "effect";
import type { Result } from "effect";

import {
  type FlickHashMap,
  HashKey,
  hashMapFromSeq,
  hashMapOf,
  hashMapSetReplace,
  type Witness,
} from "../stdlib/hash.ts";

export * from "effect/HashMap";

type Hm<K, V> = FlickHashMap<K, V>;

const box = <K, V>(m: Hm<K, V>, k: K): HashKey<K> => new HashKey(k, m.witness);
const keep = <K, V, B>(m: Hm<K, V>, native: HashMap.HashMap<HashKey<K>, B>): Hm<K, B> => hashMapOf(m.witness, native);

export const flickFromList = <K, V>(w: Witness<K>, entries: ReadonlyArray<readonly [K, V]>): Hm<K, V> =>
  hashMapFromSeq(w, entries);
export const flickIsEmpty = <K, V>(m: Hm<K, V>): boolean => HashMap.isEmpty(m.native);
export const flickHas = <K, V>(k: K, m: Hm<K, V>): boolean => HashMap.has(m.native, box(m, k));
export const flickGetHash = <K, V>(k: K, hash: number, m: Hm<K, V>): Option.Option<V> =>
  HashMap.getHash(m.native, box(m, k), hash);
export const flickHasHash = <K, V>(k: K, hash: number, m: Hm<K, V>): boolean => HashMap.hasHash(m.native, box(m, k), hash);
export const flickHasBy = <K, V>(f: (v: V, k: K) => boolean, m: Hm<K, V>): boolean =>
  HashMap.hasBy(m.native, (v, k) => f(v, k.value));
export const flickKeys = <K, V>(m: Hm<K, V>): Array<K> => Array.from(HashMap.keys(m.native), (k) => k.value);
export const flickValues = <K, V>(m: Hm<K, V>): Array<V> => HashMap.toValues(m.native);
const REPLACE_HASH_SENTINEL = /*#__PURE__*/ Symbol("flick/hashmap-modify-hash-replace");

export const flickModifyAt = <K, V>(k: K, f: (v: Option.Option<V>) => Option.Option<V>, m: Hm<K, V>): Hm<K, V> => {
  const key = box(m, k);
  const next = f(HashMap.get(m.native, key));
  return keep(m, Option.isSome(next) ? hashMapSetReplace(m.native, key, next.value) : HashMap.remove(m.native, key));
};
export const flickModifyHash = <K, V>(
  k: K,
  hash: number,
  f: (v: Option.Option<V>) => Option.Option<V>,
  m: Hm<K, V>,
): Hm<K, V> => {
  const key = box(m, k);
  const existing = HashMap.getHash(m.native, key, hash);
  const next = f(existing);
  if (Option.isNone(next)) return keep(m, HashMap.modifyHash(m.native, key, hash, () => Option.none()));
  if (Option.isSome(existing) && Object.is(existing.value, next.value)) {
    const cleared = HashMap.modifyHash(
      m.native,
      key,
      hash,
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- transient sentinel overwritten immediately on the next line
      () => Option.some(REPLACE_HASH_SENTINEL as unknown as V),
    );
    return keep(m, HashMap.modifyHash(cleared, key, hash, () => next));
  }
  return keep(m, HashMap.modifyHash(m.native, key, hash, () => next));
};
export const flickModify = <K, V>(k: K, f: (v: V) => V, m: Hm<K, V>): Hm<K, V> => {
  const key = box(m, k);
  const existing = HashMap.get(m.native, key);
  return Option.isSome(existing) ? keep(m, hashMapSetReplace(m.native, key, f(existing.value))) : m;
};
export const flickRemoveMany = <K, V>(ks: ReadonlyArray<K>, m: Hm<K, V>): Hm<K, V> =>
  keep(
    m,
    HashMap.removeMany(
      m.native,
      ks.map((k) => box(m, k)),
    ),
  );
export const flickSetMany = <K, V>(entries: ReadonlyArray<readonly [K, V]>, m: Hm<K, V>): Hm<K, V> => {
  let out = m.native;
  for (const [k, v] of entries) out = hashMapSetReplace(out, box(m, k), v);
  return keep(m, out);
};
export const flickMap = <K, V, B>(f: (v: V, k: K) => B, m: Hm<K, V>): Hm<K, B> =>
  keep(
    m,
    HashMap.map(m.native, (v, k) => f(v, k.value)),
  );
export const flickFlatMap = <K, V, B>(f: (v: V, k: K) => Hm<K, B>, m: Hm<K, V>): Hm<K, B> =>
  keep(
    m,
    HashMap.flatMap(m.native, (v, k) => f(v, k.value).native),
  );
export const flickReduce = <K, V, Z>(zero: Z, f: (z: Z, v: V, k: K) => Z, m: Hm<K, V>): Z =>
  HashMap.reduce(m.native, zero, (z, v, k) => f(z, v, k.value));
export const flickFilter = <K, V>(f: (v: V, k: K) => boolean, m: Hm<K, V>): Hm<K, V> =>
  keep(
    m,
    HashMap.filter(m.native, (v, k) => f(v, k.value)),
  );
export const flickCompact = <K, V>(m: Hm<K, Option.Option<V>>): Hm<K, V> => keep(m, HashMap.compact(m.native));
export const flickFilterMap = <K, V, B, X>(f: (v: V, k: K) => Result.Result<B, X>, m: Hm<K, V>): Hm<K, B> =>
  keep(
    m,
    HashMap.filterMap(m.native, (v, k) => f(v, k.value)),
  );
export const flickFindFirst = <K, V>(f: (v: V, k: K) => boolean, m: Hm<K, V>): Option.Option<[K, V]> =>
  Option.map(
    HashMap.findFirst(m.native, (v, k) => f(v, k.value)),
    ([k, v]): [K, V] => [k.value, v],
  );
export const flickSome = <K, V>(f: (v: V, k: K) => boolean, m: Hm<K, V>): boolean =>
  HashMap.some(m.native, (v, k) => f(v, k.value));
export const flickEvery = <K, V>(f: (v: V, k: K) => boolean, m: Hm<K, V>): boolean =>
  HashMap.every(m.native, (v, k) => f(v, k.value));
