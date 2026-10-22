// Hash sets and maps over Effect's collections. Each element or key is
// wrapped with the witness of its collection, which gives Effect's Equal and
// Hash protocols the witness's `eq` and `hash`.
import { Equal, Hash, HashMap, HashSet } from "effect";

import { none, some } from "../data.js";

class Key {
  constructor(value, witness) {
    this.value = value;
    this.witness = witness;
    Object.freeze(this);
  }
  [Equal.symbol](that) {
    return that instanceof Key && this.witness.eq(this.value, that.value);
  }
  [Hash.symbol]() {
    return this.witness.hash(this.value);
  }
}

const witness = (eq, hash) => Object.freeze({ eq, hash });

/** `EqHash.with eq hash body`: `body` receives a witness of a fresh identity. */
export const eqHashWith = (eq, hash, body) => body(witness(eq, hash));

export const eqHashCanonical = (dHash) => witness((a, b) => dHash.$Eq.eq(a)(b), dHash.hash);

const set = (w, native) => Object.freeze({ witness: w, native });

export const hashSetWith = (w) => set(w, HashSet.empty());
export const hashSetEmpty = (dHash) => hashSetWith(eqHashCanonical(dHash));
export const hashSetFromList = (w, xs) => set(w, HashSet.fromIterable(xs.map((x) => new Key(x, w))));
export const hashSetInsert = (x, s) => set(s.witness, HashSet.add(s.native, new Key(x, s.witness)));
export const hashSetRemove = (x, s) => set(s.witness, HashSet.remove(s.native, new Key(x, s.witness)));
export const hashSetMember = (x, s) => HashSet.has(s.native, new Key(x, s.witness));
export const hashSetSize = (s) => HashSet.size(s.native);
export const hashSetToList = (s) => Array.from(s.native, (k) => k.value);
export const hashSetUnion = (a, b) => set(a.witness, HashSet.union(a.native, b.native));
export const hashSetRehash = (w, s) => hashSetFromList(w, hashSetToList(s));

const map = (w, native) => Object.freeze({ witness: w, native });

export const hashMapWith = (w) => map(w, HashMap.empty());
export const hashMapEmpty = (dHash) => hashMapWith(eqHashCanonical(dHash));
export const hashMapInsert = (k, v, m) => map(m.witness, HashMap.set(m.native, new Key(k, m.witness), v));
export const hashMapRemove = (k, m) => map(m.witness, HashMap.remove(m.native, new Key(k, m.witness)));
export const hashMapGet = (k, m) => {
  const found = HashMap.get(m.native, new Key(k, m.witness));
  return found._tag === "Some" ? some(found.value) : none;
};
export const hashMapSize = (m) => HashMap.size(m.native);
export const hashMapToList = (m) => Array.from(m.native, ([k, v]) => [k.value, v]);
export const hashMapUnion = (a, b) => map(a.witness, HashMap.union(a.native, b.native));
export const hashMapRehash = (w, m) =>
  map(w, HashMap.fromIterable(hashMapToList(m).map(([k, v]) => [new Key(k, w), v])));
