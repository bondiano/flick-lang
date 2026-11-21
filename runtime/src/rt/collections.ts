// Persistent hash collections without Effect. Entries retain the witness that
// defines equality and hashing; a persistent 5-bit HAMT walks hashes from low
// fragments to high, with insertion order for collisions.
import { type HashDict, type Option, none, some } from "../core/data.ts";
import { type Seq, seq } from "../core/seq.ts";
import { item } from "../internal.ts";

export interface Witness<A> {
  readonly eq: (a: A, b: A) => boolean;
  readonly hash: (a: A) => number;
}

export const eqHashWith = <A, B>(eq: (a: A, b: A) => boolean, hash: (a: A) => number, body: (w: Witness<A>) => B): B =>
  body(Object.freeze({ eq, hash }));

export const eqHashCanonical = <A>(dict: HashDict<A>): Witness<A> =>
  Object.freeze({ eq: (a: A, b: A) => dict.$Eq.eq(a)(b), hash: dict.hash });

export interface Entry<K, V> {
  readonly key: K;
  readonly value: V;
  readonly hash: number;
}

export interface LeafNode<K, V> {
  readonly tag: "leaf";
  readonly hash: number;
  readonly entries: ReadonlyArray<Entry<K, V>>;
}

export interface BranchNode<K, V> {
  readonly tag: "branch";
  readonly bitmap: number;
  readonly children: ReadonlyArray<HNode<K, V>>;
}

export type HNode<K, V> = LeafNode<K, V> | BranchNode<K, V>;

interface MutLeafNode<K, V> {
  readonly tag: "leaf";
  readonly hash: number;
  readonly entries: Array<Entry<K, V>>;
}

interface MutBranchNode<K, V> {
  readonly tag: "branch";
  bitmap: number;
  readonly children: Array<MutHNode<K, V>>;
}

type MutHNode<K, V> = MutLeafNode<K, V> | MutBranchNode<K, V>;

interface ChangeFlag {
  changed: boolean;
}

const frag = (hash: number, shift: number): number => (hash >>> shift) & 31;

const popcount = (n: number): number => {
  n -= (n >>> 1) & 0x55555555;
  n = (n & 0x33333333) + ((n >>> 2) & 0x33333333);
  return (((n + (n >>> 4)) & 0x0f0f0f0f) * 0x01010101) >>> 24;
};

const slot = (bitmap: number, fragment: number): number =>
  popcount(bitmap & (((1 << fragment) >>> 0) - 1));

const insertAt = <T>(xs: ReadonlyArray<T>, index: number, value: T): Array<T> => {
  const out = xs.slice();
  out.splice(index, 0, value);
  return out;
};

const removeAt = <T>(xs: ReadonlyArray<T>, index: number): Array<T> => {
  const out = xs.slice();
  out.splice(index, 1);
  return out;
};

const findNode = <K, V>(
  root: HNode<K, V> | undefined,
  witness: Witness<K>,
  key: K,
  hash: number,
): Entry<K, V> | undefined => {
  let node = root;
  let shift = 0;
  while (node !== undefined) {
    if (node.tag === "leaf") {
      if (node.hash !== hash) return undefined;
      for (let i = 0; i < node.entries.length; i++) {
        const entry = item(node.entries, i);
        if (witness.eq(entry.key, key)) return entry;
      }
      return undefined;
    }
    const f = frag(hash, shift);
    if ((node.bitmap & (1 << f)) === 0) return undefined;
    node = item(node.children, slot(node.bitmap, f));
    shift += 5;
  }
  return undefined;
};

const insertNode = <K, V>(
  node: HNode<K, V> | undefined,
  witness: Witness<K>,
  entry: Entry<K, V>,
  shift: number,
  keep: boolean,
  added: ChangeFlag,
): HNode<K, V> => {
  if (node === undefined) {
    added.changed = true;
    return { tag: "leaf", hash: entry.hash, entries: [entry] };
  }
  if (node.tag === "leaf") {
    if (node.hash === entry.hash) {
      for (let i = 0; i < node.entries.length; i++) {
        const existing = item(node.entries, i);
        if (!witness.eq(existing.key, entry.key)) continue;
        if (keep) return node;
        const entries = node.entries.slice();
        entries[i] = entry;
        return { tag: "leaf", hash: node.hash, entries };
      }
      added.changed = true;
      return { tag: "leaf", hash: node.hash, entries: [...node.entries, entry] };
    }
    const f1 = frag(node.hash, shift);
    const f2 = frag(entry.hash, shift);
    if (f1 === f2) {
      const child = insertNode(node, witness, entry, shift + 5, keep, added);
      return { tag: "branch", bitmap: 1 << f1, children: [child] };
    }
    const leaf: LeafNode<K, V> = { tag: "leaf", hash: entry.hash, entries: [entry] };
    added.changed = true;
    return { tag: "branch", bitmap: (1 << f1) | (1 << f2), children: f1 < f2 ? [node, leaf] : [leaf, node] };
  }
  const f = frag(entry.hash, shift);
  const bit = 1 << f;
  const i = slot(node.bitmap, f);
  if ((node.bitmap & bit) === 0) {
    const leaf: LeafNode<K, V> = { tag: "leaf", hash: entry.hash, entries: [entry] };
    added.changed = true;
    return { tag: "branch", bitmap: node.bitmap | bit, children: insertAt(node.children, i, leaf) };
  }
  const existingChild = item(node.children, i);
  const kid = insertNode(existingChild, witness, entry, shift + 5, keep, added);
  if (kid === existingChild) return node;
  const children = node.children.slice();
  children[i] = kid;
  return { tag: "branch", bitmap: node.bitmap, children };
};

// Mutates only freshly allocated nodes owned by a single construction pass.
const insertNodeMut = <K, V>(
  node: MutHNode<K, V> | undefined,
  witness: Witness<K>,
  entry: Entry<K, V>,
  shift: number,
  keep: boolean,
  added: ChangeFlag,
): MutHNode<K, V> => {
  if (node === undefined) {
    added.changed = true;
    return { tag: "leaf", hash: entry.hash, entries: [entry] };
  }
  if (node.tag === "leaf") {
    if (node.hash === entry.hash) {
      for (let i = 0; i < node.entries.length; i++) {
        const existing = item(node.entries, i);
        if (!witness.eq(existing.key, entry.key)) continue;
        if (!keep) node.entries[i] = entry;
        return node;
      }
      node.entries.push(entry);
      added.changed = true;
      return node;
    }
    const f1 = frag(node.hash, shift);
    const f2 = frag(entry.hash, shift);
    if (f1 === f2) {
      const child = insertNodeMut(node, witness, entry, shift + 5, keep, added);
      return { tag: "branch", bitmap: 1 << f1, children: [child] };
    }
    const leaf: MutLeafNode<K, V> = { tag: "leaf", hash: entry.hash, entries: [entry] };
    added.changed = true;
    return { tag: "branch", bitmap: (1 << f1) | (1 << f2), children: f1 < f2 ? [node, leaf] : [leaf, node] };
  }
  const f = frag(entry.hash, shift);
  const bit = 1 << f;
  const i = slot(node.bitmap, f);
  if ((node.bitmap & bit) === 0) {
    const leaf: MutLeafNode<K, V> = { tag: "leaf", hash: entry.hash, entries: [entry] };
    node.children.splice(i, 0, leaf);
    node.bitmap |= bit;
    added.changed = true;
    return node;
  }
  node.children[i] = insertNodeMut(item(node.children, i), witness, entry, shift + 5, keep, added);
  return node;
};

const removeNode = <K, V>(
  node: HNode<K, V> | undefined,
  witness: Witness<K>,
  key: K,
  hash: number,
  shift: number,
  removed: ChangeFlag,
): HNode<K, V> | undefined => {
  if (node === undefined) return undefined;
  if (node.tag === "leaf") {
    if (node.hash !== hash) return node;
    for (let i = 0; i < node.entries.length; i++) {
      if (!witness.eq(item(node.entries, i).key, key)) continue;
      removed.changed = true;
      if (node.entries.length === 1) return undefined;
      return { tag: "leaf", hash: node.hash, entries: removeAt(node.entries, i) };
    }
    return node;
  }
  const f = frag(hash, shift);
  const bit = 1 << f;
  if ((node.bitmap & bit) === 0) return node;
  const i = slot(node.bitmap, f);
  const kid = removeNode(item(node.children, i), witness, key, hash, shift + 5, removed);
  if (!removed.changed) return node;
  if (kid !== undefined) {
    const children = node.children.slice();
    children[i] = kid;
    return { tag: "branch", bitmap: node.bitmap, children };
  }
  if (node.children.length === 1) return undefined;
  const children = removeAt(node.children, i);
  if (children.length === 1) {
    const only = item(children, 0);
    if (only.tag === "leaf") return only;
  }
  return { tag: "branch", bitmap: node.bitmap & ~bit, children };
};

const collectEntries = <K, V>(node: HNode<K, V> | undefined, out: Array<[K, V]>): void => {
  if (node === undefined) return;
  if (node.tag === "leaf") {
    for (let i = 0; i < node.entries.length; i++) {
      const entry = item(node.entries, i);
      out.push([entry.key, entry.value]);
    }
    return;
  }
  for (let i = 0; i < node.children.length; i++) collectEntries(item(node.children, i), out);
};

const collectKeys = <K, V>(node: HNode<K, V> | undefined, out: Array<K>): void => {
  if (node === undefined) return;
  if (node.tag === "leaf") {
    for (let i = 0; i < node.entries.length; i++) out.push(item(node.entries, i).key);
    return;
  }
  for (let i = 0; i < node.children.length; i++) collectKeys(item(node.children, i), out);
};

function* walkMap<K, V>(root: HNode<K, V> | undefined): Generator<[K, V], void, undefined> {
  if (root === undefined) return;
  const stack: Array<HNode<K, V>> = [root];
  while (stack.length > 0) {
    const node = stack.pop();
    if (node === undefined) continue;
    if (node.tag === "leaf") {
      const { entries } = node;
      for (let i = 0; i < entries.length; i++) {
        const entry = item(entries, i);
        yield [entry.key, entry.value];
      }
    } else {
      const { children } = node;
      for (let i = children.length - 1; i >= 0; i--) stack.push(item(children, i));
    }
  }
}

function* walkKeys<K, V>(root: HNode<K, V> | undefined): Generator<K, void, undefined> {
  if (root === undefined) return;
  const stack: Array<HNode<K, V>> = [root];
  while (stack.length > 0) {
    const node = stack.pop();
    if (node === undefined) continue;
    if (node.tag === "leaf") {
      const { entries } = node;
      for (let i = 0; i < entries.length; i++) yield item(entries, i).key;
    } else {
      const { children } = node;
      for (let i = children.length - 1; i >= 0; i--) stack.push(item(children, i));
    }
  }
}

export class FlickHashMap<K, V> {
  readonly witness: Witness<K>;
  readonly root: HNode<K, V> | undefined;
  readonly size: number;

  constructor(witness: Witness<K>, root: HNode<K, V> | undefined, size: number) {
    this.witness = witness;
    this.root = root;
    this.size = size;
  }
}

export const isFlickHashMap = (value: unknown): value is FlickHashMap<unknown, unknown> =>
  value instanceof FlickHashMap;

const insertMap = <K, V>(key: K, hash: number, value: V, m: FlickHashMap<K, V>, keep: boolean): FlickHashMap<K, V> => {
  const added: ChangeFlag = { changed: false };
  const root = insertNode(m.root, m.witness, { key, value, hash: hash | 0 }, 0, keep, added);
  return root === m.root ? m : new FlickHashMap(m.witness, root, m.size + (added.changed ? 1 : 0));
};

const buildMap = <K, V>(witness: Witness<K>, entries: Iterable<readonly [K, V]>): FlickHashMap<K, V> => {
  let root: MutHNode<K, V> | undefined;
  let size = 0;
  const added: ChangeFlag = { changed: false };
  for (const [key, value] of entries) {
    added.changed = false;
    root = insertNodeMut(root, witness, { key, value, hash: witness.hash(key) | 0 }, 0, false, added);
    if (added.changed) size++;
  }
  return new FlickHashMap(witness, root, size);
};

const buildSetMap = <A>(witness: Witness<A>, values: Iterable<A>): FlickHashMap<A, true> => {
  let root: MutHNode<A, true> | undefined;
  let size = 0;
  const added: ChangeFlag = { changed: false };
  for (const key of values) {
    added.changed = false;
    root = insertNodeMut(root, witness, { key, value: true, hash: witness.hash(key) | 0 }, 0, true, added);
    if (added.changed) size++;
  }
  return new FlickHashMap(witness, root, size);
};

export const hashMapWith = <K, V>(witness: Witness<K>): FlickHashMap<K, V> => new FlickHashMap(witness, undefined, 0);
export const hashMapEmpty = <K, V>(dict: HashDict<K>): FlickHashMap<K, V> => hashMapWith(eqHashCanonical(dict));
export const hashMapInsert = <K, V>(key: K, value: V, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  insertMap(key, m.witness.hash(key), value, m, false);
export const hashMapInsertHash = <K, V>(key: K, hash: number, value: V, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  insertMap(key, hash, value, m, false);
export const hashMapRemove = <K, V>(key: K, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  hashMapRemoveHash(key, m.witness.hash(key), m);
export const hashMapRemoveHash = <K, V>(key: K, hash: number, m: FlickHashMap<K, V>): FlickHashMap<K, V> => {
  const removed: ChangeFlag = { changed: false };
  const root = removeNode(m.root, m.witness, key, hash | 0, 0, removed);
  return removed.changed ? new FlickHashMap(m.witness, root, m.size - 1) : m;
};
export const hashMapGetHash = <K, V>(key: K, hash: number, m: FlickHashMap<K, V>): Option<V> => {
  const found = findNode(m.root, m.witness, key, hash | 0);
  return found === undefined ? none : some(found.value);
};
export const hashMapGet = <K, V>(key: K, m: FlickHashMap<K, V>): Option<V> =>
  hashMapGetHash(key, m.witness.hash(key), m);
export const hashMapHasHash = <K, V>(key: K, hash: number, m: FlickHashMap<K, V>): boolean =>
  findNode(m.root, m.witness, key, hash | 0) !== undefined;
export const hashMapHas = <K, V>(key: K, m: FlickHashMap<K, V>): boolean => hashMapHasHash(key, m.witness.hash(key), m);
export const hashMapSize = <K, V>(m: FlickHashMap<K, V>): number => m.size;
export const hashMapToList = <K, V>(m: FlickHashMap<K, V>): Array<[K, V]> => {
  const out: Array<[K, V]> = [];
  collectEntries(m.root, out);
  return out;
};
export const hashMapToSeq = <K, V>(m: FlickHashMap<K, V>): Seq<[K, V]> => seq(() => walkMap(m.root));
export const hashMapFromSeq = <K, V>(witness: Witness<K>, entries: Seq<readonly [K, V]>): FlickHashMap<K, V> =>
  buildMap(witness, entries);
export const hashMapUnion = <K, V>(a: FlickHashMap<K, V>, b: FlickHashMap<K, V>): FlickHashMap<K, V> => {
  let result = a;
  for (const [key, value] of walkMap(b.root)) result = hashMapInsert(key, value, result);
  return result;
};
export const hashMapRehash = <K, V>(witness: Witness<K>, m: FlickHashMap<K, V>): FlickHashMap<K, V> =>
  buildMap(witness, walkMap(m.root));
export const hashMapFromList = <K, V>(witness: Witness<K>, entries: ReadonlyArray<readonly [K, V]>): FlickHashMap<K, V> =>
  buildMap(witness, entries);

export class FlickHashSet<A> {
  readonly witness: Witness<A>;
  readonly map: FlickHashMap<A, true>;

  constructor(map: FlickHashMap<A, true>) {
    this.witness = map.witness;
    this.map = map;
  }
}

export const isFlickHashSet = (value: unknown): value is FlickHashSet<unknown> =>
  value instanceof FlickHashSet;

const setOf = <A>(map: FlickHashMap<A, true>): FlickHashSet<A> => new FlickHashSet(map);
export const hashSetWith = <A>(witness: Witness<A>): FlickHashSet<A> => setOf(hashMapWith(witness));
export const hashSetEmpty = <A>(dict: HashDict<A>): FlickHashSet<A> => hashSetWith(eqHashCanonical(dict));
export const hashSetFromList = <A>(witness: Witness<A>, values: ReadonlyArray<A>): FlickHashSet<A> =>
  setOf(buildSetMap(witness, values));
export const hashSetFromSeq = <A>(witness: Witness<A>, values: Seq<A>): FlickHashSet<A> =>
  setOf(buildSetMap(witness, values));
export const hashSetInsert = <A>(value: A, set: FlickHashSet<A>): FlickHashSet<A> => {
  const next = insertMap(value, set.witness.hash(value), true, set.map, true);
  return next === set.map ? set : setOf(next);
};
export const hashSetRemove = <A>(value: A, set: FlickHashSet<A>): FlickHashSet<A> => {
  const next = hashMapRemove(value, set.map);
  return next === set.map ? set : setOf(next);
};
export const hashSetMember = <A>(value: A, set: FlickHashSet<A>): boolean => hashMapHas(value, set.map);
export const hashSetSize = <A>(set: FlickHashSet<A>): number => hashMapSize(set.map);
export const hashSetToList = <A>(set: FlickHashSet<A>): Array<A> => {
  const out: Array<A> = [];
  collectKeys(set.map.root, out);
  return out;
};
export const hashSetToSeq = <A>(set: FlickHashSet<A>): Seq<A> => seq(() => walkKeys(set.map.root));
export const hashSetUnion = <A>(a: FlickHashSet<A>, b: FlickHashSet<A>): FlickHashSet<A> => {
  let map = a.map;
  for (const value of walkKeys(b.map.root)) map = insertMap(value, map.witness.hash(value), true, map, true);
  return map === a.map ? a : setOf(map);
};
export const hashSetRehash = <A>(witness: Witness<A>, set: FlickHashSet<A>): FlickHashSet<A> =>
  setOf(buildSetMap(witness, walkKeys(set.map.root)));
