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
  hashMapToSeq,
  hashMapWith,
} from "../collections.ts";

type Map<K, V> = FlickHashMap<K, V>;

export const flickFromList = hashMapFromList;
export const flickIsEmpty = <K, V>(m: Map<K, V>): boolean => hashMapSize(m) === 0;
export const flickHas = hashMapHas;
export const flickGetHash = hashMapGetHash;
export const flickHasHash = hashMapHasHash;
export const flickHasBy = <K, V>(f: (value: V, key: K) => boolean, m: Map<K, V>): boolean => {
  for (const [key, value] of hashMapToSeq(m)) if (f(value, key)) return true;
  return false;
};
export const flickKeys = <K, V>(m: Map<K, V>): Array<K> => {
  const out: Array<K> = [];
  for (const [key] of hashMapToSeq(m)) out.push(key);
  return out;
};
export const flickValues = <K, V>(m: Map<K, V>): Array<V> => {
  const out: Array<V> = [];
  for (const [, value] of hashMapToSeq(m)) out.push(value);
  return out;
};
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
export const flickMap = <K, V, B>(f: (value: V, key: K) => B, m: Map<K, V>): Map<K, B> => {
  let out = hashMapWith<K, B>(m.witness);
  for (const [key, value] of hashMapToSeq(m)) out = hashMapInsert(key, f(value, key), out);
  return out;
};
export const flickFlatMap = <K, V, B>(f: (value: V, key: K) => Map<K, B>, m: Map<K, V>): Map<K, B> => {
  let out = hashMapWith<K, B>(m.witness);
  for (const [key, value] of hashMapToSeq(m)) {
    for (const [innerKey, innerValue] of hashMapToSeq(f(value, key))) out = hashMapInsert(innerKey, innerValue, out);
  }
  return out;
};
export const flickReduce = <K, V, Z>(zero: Z, f: (acc: Z, value: V, key: K) => Z, m: Map<K, V>): Z => {
  let acc = zero;
  for (const [key, value] of hashMapToSeq(m)) acc = f(acc, value, key);
  return acc;
};
export const flickFilter = <K, V>(f: (value: V, key: K) => boolean, m: Map<K, V>): Map<K, V> => {
  let out = hashMapWith<K, V>(m.witness);
  for (const [key, value] of hashMapToSeq(m)) if (f(value, key)) out = hashMapInsert(key, value, out);
  return out;
};
export const flickCompact = <K, V>(m: Map<K, Option<V>>): Map<K, V> => {
  let out = hashMapWith<K, V>(m.witness);
  for (const [key, value] of hashMapToSeq(m)) if (value._tag === "Some") out = hashMapInsert(key, value.value, out);
  return out;
};
export const flickFilterMap = <K, V, B>(
  f: (value: V, key: K) => { _tag: "Success"; success: B } | { _tag: "Failure"; failure: unknown },
  m: Map<K, V>,
): Map<K, B> => {
  let out = hashMapWith<K, B>(m.witness);
  for (const [key, value] of hashMapToSeq(m)) {
    const result = f(value, key);
    if (result._tag === "Success") out = hashMapInsert(key, result.success, out);
  }
  return out;
};
export const flickFindFirst = <K, V>(f: (value: V, key: K) => boolean, m: Map<K, V>): Option<[K, V]> => {
  for (const [key, value] of hashMapToSeq(m)) if (f(value, key)) return some([key, value]);
  return none;
};
export const flickSome = flickHasBy;
export const flickEvery = <K, V>(f: (value: V, key: K) => boolean, m: Map<K, V>): boolean => {
  for (const [key, value] of hashMapToSeq(m)) if (!f(value, key)) return false;
  return true;
};
