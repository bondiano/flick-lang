// The standard HashMap bindings over Flick's own persistent hash map.
import { type Option, none, some } from "../../core/data.ts";
import {
  type FlickHashMap,
  hashMapFromList,
  hashMapGet,
  hashMapGetHash,
  hashMapHas,
  hashMapHasHash,
  hashMapInsert,
  hashMapInsertHash,
  hashMapRemove,
  hashMapRemoveHash,
  hashMapSize,
  hashMapToList,
} from "../collections.ts";

type Map<K, V> = FlickHashMap<K, V>;
const each = <K, V>(m: Map<K, V>): Array<[K, V]> => hashMapToList(m);
const rebuild = <K, V, B>(m: Map<K, V>, entries: ReadonlyArray<readonly [K, B]>): Map<K, B> =>
  hashMapFromList(m.witness, entries);

export const flickFromList = hashMapFromList;
export const flickIsEmpty = <K, V>(m: Map<K, V>): boolean => hashMapSize(m) === 0;
export const flickHas = hashMapHas;
export const flickGetHash = hashMapGetHash;
export const flickHasHash = hashMapHasHash;
export const flickHasBy = <K, V>(f: (value: V, key: K) => boolean, m: Map<K, V>): boolean =>
  each(m).some(([key, value]) => f(value, key));
export const flickKeys = <K, V>(m: Map<K, V>): Array<K> => each(m).map(([key]) => key);
export const flickValues = <K, V>(m: Map<K, V>): Array<V> => each(m).map(([, value]) => value);
export const flickModifyAt = <K, V>(key: K, f: (value: Option<V>) => Option<V>, m: Map<K, V>): Map<K, V> => {
  const next = f(hashMapGet(key, m));
  return next._tag === "Some" ? hashMapInsert(key, next.value, m) : hashMapRemove(key, m);
};
export const flickModifyHash = <K, V>(key: K, hash: number, f: (value: Option<V>) => Option<V>, m: Map<K, V>): Map<K, V> => {
  const next = f(hashMapGetHash(key, hash, m));
  return next._tag === "Some" ? hashMapInsertHash(key, hash, next.value, m) : hashMapRemoveHash(key, hash, m);
};
export const flickModify = <K, V>(key: K, f: (value: V) => V, m: Map<K, V>): Map<K, V> => {
  const found = hashMapGet(key, m);
  return found._tag === "Some" ? hashMapInsert(key, f(found.value), m) : m;
};
export const flickRemoveMany = <K, V>(keys: ReadonlyArray<K>, m: Map<K, V>): Map<K, V> =>
  keys.reduce((result, key) => hashMapRemove(key, result), m);
export const flickSetMany = <K, V>(entries: ReadonlyArray<readonly [K, V]>, m: Map<K, V>): Map<K, V> =>
  entries.reduce((result, [key, value]) => hashMapInsert(key, value, result), m);
export const flickMap = <K, V, B>(f: (value: V, key: K) => B, m: Map<K, V>): Map<K, B> =>
  rebuild(m, each(m).map(([key, value]) => [key, f(value, key)]));
export const flickFlatMap = <K, V, B>(f: (value: V, key: K) => Map<K, B>, m: Map<K, V>): Map<K, B> =>
  rebuild(m, each(m).flatMap(([key, value]) => each(f(value, key))));
export const flickReduce = <K, V, Z>(zero: Z, f: (acc: Z, value: V, key: K) => Z, m: Map<K, V>): Z =>
  each(m).reduce((acc, [key, value]) => f(acc, value, key), zero);
export const flickFilter = <K, V>(f: (value: V, key: K) => boolean, m: Map<K, V>): Map<K, V> =>
  rebuild(m, each(m).filter(([key, value]) => f(value, key)));
export const flickCompact = <K, V>(m: Map<K, Option<V>>): Map<K, V> =>
  rebuild(m, each(m).flatMap(([key, value]) => value._tag === "Some" ? [[key, value.value]] : []));
export const flickFilterMap = <K, V, B>(f: (value: V, key: K) => { _tag: "Success"; success: B } | { _tag: "Failure"; failure: unknown }, m: Map<K, V>): Map<K, B> =>
  rebuild(m, each(m).flatMap(([key, value]) => {
    const result = f(value, key);
    return result._tag === "Success" ? [[key, result.success]] : [];
  }));
export const flickFindFirst = <K, V>(f: (value: V, key: K) => boolean, m: Map<K, V>): Option<[K, V]> => {
  const found = each(m).find(([key, value]) => f(value, key));
  return found === undefined ? none : some(found);
};
export const flickSome = flickHasBy;
export const flickEvery = <K, V>(f: (value: V, key: K) => boolean, m: Map<K, V>): boolean =>
  each(m).every(([key, value]) => f(value, key));
