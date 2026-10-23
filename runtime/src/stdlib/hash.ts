// Hash sets and maps over Effect's collections. Each element or key is
// wrapped with the witness of its collection, which gives Effect's Equal and
// Hash protocols the witness's `eq` and `hash`.
import { Equal, Hash, HashMap, HashSet, Option } from "effect";

import { type HashDict, none, some } from "../data.ts";

/** `EqHash.Witness w a`: the equality and hash of a collection. */
export interface Witness<A> {
  readonly eq: (a: A, b: A) => boolean;
  readonly hash: (a: A) => number;
}

export class Key<A> implements Equal.Equal {
  readonly value: A;
  readonly witness: Witness<A>;
  constructor(value: A, witness: Witness<A>) {
    this.value = value;
    this.witness = witness;
    Object.freeze(this);
  }
  [Equal.symbol](that: Equal.Equal): boolean {
    if (!(that instanceof Key)) return false;
    // keys meet only inside collections of one type `HashSet w a`: their values are both `a`
    // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- one collection type, one element type
    return this.witness.eq(this.value, (that as Key<A>).value);
  }
  [Hash.symbol](): number {
    return this.witness.hash(this.value);
  }
}

const witness = <A>(eq: (a: A, b: A) => boolean, hash: (a: A) => number): Witness<A> => Object.freeze({ eq, hash });

/** `EqHash.with eq hash body`: `body` receives a witness of a fresh identity. */
export const eqHashWith = <A, B>(eq: (a: A, b: A) => boolean, hash: (a: A) => number, body: (w: Witness<A>) => B): B =>
  body(witness(eq, hash));

export const eqHashCanonical = <A>(dHash: HashDict<A>): Witness<A> => witness((a, b) => dHash.$Eq.eq(a)(b), dHash.hash);

/** `HashSet w a`. */
export interface FlickHashSet<A> {
  readonly witness: Witness<A>;
  readonly native: HashSet.HashSet<Key<A>>;
}

const set = <A>(w: Witness<A>, native: HashSet.HashSet<Key<A>>): FlickHashSet<A> => Object.freeze({ witness: w, native });

export const hashSetWith = <A>(w: Witness<A>): FlickHashSet<A> => set(w, HashSet.empty());
export const hashSetEmpty = <A>(dHash: HashDict<A>): FlickHashSet<A> => hashSetWith(eqHashCanonical(dHash));
export const hashSetFromList = <A>(w: Witness<A>, xs: ReadonlyArray<A>): FlickHashSet<A> =>
  set(w, HashSet.fromIterable(xs.map((x) => new Key(x, w))));
export const hashSetInsert = <A>(x: A, s: FlickHashSet<A>): FlickHashSet<A> =>
  set(s.witness, HashSet.add(s.native, new Key(x, s.witness)));
export const hashSetRemove = <A>(x: A, s: FlickHashSet<A>): FlickHashSet<A> =>
  set(s.witness, HashSet.remove(s.native, new Key(x, s.witness)));
export const hashSetMember = <A>(x: A, s: FlickHashSet<A>): boolean => HashSet.has(s.native, new Key(x, s.witness));
export const hashSetSize = <A>(s: FlickHashSet<A>): number => HashSet.size(s.native);
export const hashSetToList = <A>(s: FlickHashSet<A>): Array<A> => Array.from(s.native, (k) => k.value);
export const hashSetUnion = <A>(a: FlickHashSet<A>, b: FlickHashSet<A>): FlickHashSet<A> =>
  set(a.witness, HashSet.union(a.native, b.native));
export const hashSetRehash = <A>(w: Witness<A>, s: FlickHashSet<A>): FlickHashSet<A> => hashSetFromList(w, hashSetToList(s));

/** `HashMap w k v`. */
export interface FlickHashMap<K, V> {
  readonly witness: Witness<K>;
  readonly native: HashMap.HashMap<Key<K>, V>;
}

const map = <K, V>(w: Witness<K>, native: HashMap.HashMap<Key<K>, V>): FlickHashMap<K, V> =>
  Object.freeze({ witness: w, native });

export const hashMapWith = <K, V>(w: Witness<K>): FlickHashMap<K, V> => map(w, HashMap.empty());
export const hashMapEmpty = <K, V>(dHash: HashDict<K>): FlickHashMap<K, V> => hashMapWith(eqHashCanonical(dHash));
export const hashMapInsert = <K, V>(k: K, v: V, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  map(m.witness, HashMap.set(m.native, new Key(k, m.witness), v));
export const hashMapRemove = <K, V>(k: K, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  map(m.witness, HashMap.remove(m.native, new Key(k, m.witness)));
export const hashMapGet = <K, V>(k: K, m: FlickHashMap<K, V>): Option.Option<V> => {
  const found = HashMap.get(m.native, new Key(k, m.witness));
  return Option.isSome(found) ? some(found.value) : none;
};
export const hashMapSize = <K, V>(m: FlickHashMap<K, V>): number => HashMap.size(m.native);
export const hashMapToList = <K, V>(m: FlickHashMap<K, V>): Array<[K, V]> => Array.from(m.native, ([k, v]) => [k.value, v]);
export const hashMapUnion = <K, V>(a: FlickHashMap<K, V>, b: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  map(a.witness, HashMap.union(a.native, b.native));
export const hashMapRehash = <K, V>(w: Witness<K>, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  map(w, HashMap.fromIterable(hashMapToList(m).map(([k, v]) => [new Key(k, w), v] as const)));
