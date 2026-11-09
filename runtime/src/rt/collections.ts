// Persistent hash collections without Effect. Entries retain the witness that
// defines equality and hashing; iteration follows Effect's HAMT traversal:
// hash fragments from low to high, with insertion order for collisions.
import { type HashDict, type Option, none, some } from "../core/data.ts";

export interface Witness<A> {
  readonly eq: (a: A, b: A) => boolean;
  readonly hash: (a: A) => number;
}

export const eqHashWith = <A, B>(eq: (a: A, b: A) => boolean, hash: (a: A) => number, body: (w: Witness<A>) => B): B =>
  body(Object.freeze({ eq, hash }));

export const eqHashCanonical = <A>(dict: HashDict<A>): Witness<A> =>
  Object.freeze({ eq: (a: A, b: A) => dict.$Eq.eq(a)(b), hash: dict.hash });

interface Entry<K, V> {
  readonly key: K;
  readonly value: V;
  readonly hash: number;
}

export interface FlickHashMap<K, V> {
  readonly witness: Witness<K>;
  readonly entries: ReadonlyArray<Entry<K, V>>;
}

const mapOf = <K, V>(witness: Witness<K>, entries: ReadonlyArray<Entry<K, V>>): FlickHashMap<K, V> => {
  for (const entry of entries) Object.freeze(entry);
  return Object.freeze({ witness, entries: Object.freeze(entries) });
};

const indexOf = <K, V>(m: FlickHashMap<K, V>, key: K, hash: number): number =>
  m.entries.findIndex((entry) => entry.hash === hash && m.witness.eq(entry.key, key));

// An IndexedNode visits children by the unsigned position of each five-bit
// fragment. A CollisionNode visits its entries in insertion order.
const hashOrder = (left: number, right: number): number => {
  for (let shift = 0; shift < 32; shift += 5) {
    const a = (left >>> shift) & 31;
    const b = (right >>> shift) & 31;
    if (a !== b) return a - b;
  }
  return 0;
};

const ordered = <K, V>(m: FlickHashMap<K, V>): Array<Entry<K, V>> =>
  m.entries.toSorted((a, b) => hashOrder(a.hash, b.hash));

export const hashMapWith = <K, V>(witness: Witness<K>): FlickHashMap<K, V> => mapOf(witness, []);
export const hashMapEmpty = <K, V>(dict: HashDict<K>): FlickHashMap<K, V> => hashMapWith(eqHashCanonical(dict));
export const hashMapInsert = <K, V>(key: K, value: V, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  hashMapInsertHash(key, m.witness.hash(key), value, m);
export const hashMapInsertHash = <K, V>(key: K, hash: number, value: V, m: FlickHashMap<K, V>): FlickHashMap<K, V> => {
  const at = indexOf(m, key, hash);
  const entry = { key, value, hash };
  if (at < 0) return mapOf(m.witness, [...m.entries, entry]);
  const entries = [...m.entries];
  entries[at] = entry;
  return mapOf(m.witness, entries);
};
export const hashMapRemove = <K, V>(key: K, m: FlickHashMap<K, V>): FlickHashMap<K, V> => {
  return hashMapRemoveHash(key, m.witness.hash(key), m);
};
export const hashMapRemoveHash = <K, V>(key: K, hash: number, m: FlickHashMap<K, V>): FlickHashMap<K, V> => {
  const at = indexOf(m, key, hash);
  return at < 0 ? m : mapOf(m.witness, m.entries.filter((_, i) => i !== at));
};
export const hashMapGetHash = <K, V>(key: K, hash: number, m: FlickHashMap<K, V>): Option<V> => {
  const at = indexOf(m, key, hash);
  const found = m.entries[at];
  return found === undefined ? none : some(found.value);
};
export const hashMapGet = <K, V>(key: K, m: FlickHashMap<K, V>): Option<V> =>
  hashMapGetHash(key, m.witness.hash(key), m);
export const hashMapHasHash = <K, V>(key: K, hash: number, m: FlickHashMap<K, V>): boolean => indexOf(m, key, hash) >= 0;
export const hashMapHas = <K, V>(key: K, m: FlickHashMap<K, V>): boolean => hashMapHasHash(key, m.witness.hash(key), m);
export const hashMapSize = <K, V>(m: FlickHashMap<K, V>): number => m.entries.length;
export const hashMapToList = <K, V>(m: FlickHashMap<K, V>): Array<[K, V]> => ordered(m).map(({ key, value }) => [key, value]);
export const hashMapUnion = <K, V>(a: FlickHashMap<K, V>, b: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  hashMapToList(b).reduce((m, [key, value]) => hashMapInsert(key, value, m), a);
export const hashMapRehash = <K, V>(witness: Witness<K>, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  hashMapFromList(witness, hashMapToList(m));
export const hashMapFromList = <K, V>(witness: Witness<K>, entries: ReadonlyArray<readonly [K, V]>): FlickHashMap<K, V> =>
  entries.reduce((m, [key, value]) => hashMapInsert(key, value, m), hashMapWith<K, V>(witness));

export interface FlickHashSet<A> {
  readonly witness: Witness<A>;
  readonly map: FlickHashMap<A, true>;
}

const setOf = <A>(map: FlickHashMap<A, true>): FlickHashSet<A> => Object.freeze({ witness: map.witness, map });
export const hashSetWith = <A>(witness: Witness<A>): FlickHashSet<A> => setOf(hashMapWith(witness));
export const hashSetEmpty = <A>(dict: HashDict<A>): FlickHashSet<A> => hashSetWith(eqHashCanonical(dict));
export const hashSetFromList = <A>(witness: Witness<A>, values: ReadonlyArray<A>): FlickHashSet<A> =>
  values.reduce((set, value) => hashSetInsert(value, set), hashSetWith(witness));
export const hashSetInsert = <A>(value: A, set: FlickHashSet<A>): FlickHashSet<A> => setOf(hashMapInsert(value, true, set.map));
export const hashSetRemove = <A>(value: A, set: FlickHashSet<A>): FlickHashSet<A> => setOf(hashMapRemove(value, set.map));
export const hashSetMember = <A>(value: A, set: FlickHashSet<A>): boolean => hashMapHas(value, set.map);
export const hashSetSize = <A>(set: FlickHashSet<A>): number => hashMapSize(set.map);
export const hashSetToList = <A>(set: FlickHashSet<A>): Array<A> => hashMapToList(set.map).map(([key]) => key);
export const hashSetUnion = <A>(a: FlickHashSet<A>, b: FlickHashSet<A>): FlickHashSet<A> => setOf(hashMapUnion(a.map, b.map));
export const hashSetRehash = <A>(witness: Witness<A>, set: FlickHashSet<A>): FlickHashSet<A> =>
  hashSetFromList(witness, hashSetToList(set));
