import assert from "node:assert/strict";
import { test } from "node:test";

import { seq, seqFirst, seqRange, seqTake, seqToList, type Seq } from "../src/core/seq.ts";
import {
  FlickHashMap,
  FlickHashSet,
  type HNode,
  hashMapFromList,
  hashMapFromSeq,
  hashMapGet,
  hashMapInsert,
  hashMapRemove,
  hashMapSize,
  hashMapToList,
  hashMapToSeq,
  hashMapWith,
  hashSetFromList,
  hashSetFromSeq,
  hashSetInsert,
  hashSetMember,
  hashSetRemove,
  hashSetSize,
  hashSetToList,
  hashSetToSeq,
  hashSetWith,
} from "../src/rt/collections.ts";
import { hash } from "../src/rt/hash.ts";

const intWitness = { eq: (a: number, b: number) => a === b, hash: (n: number) => n };

const singlePass = <A>(items: ReadonlyArray<A>): { readonly seq: Seq<A>; readonly pulls: () => number; readonly runs: () => number } => {
  let runs = 0;
  let pulls = 0;
  return {
    seq: seq(function* () {
      runs++;
      assert.equal(runs, 1, "fromSeq must consume finite input only once");
      for (const item of items) {
        pulls++;
        yield item;
      }
    }),
    pulls: () => pulls,
    runs: () => runs,
  };
};

const instrumentNode = <K, V>(
  node: HNode<K, V>,
  onVisit: (tag: HNode<K, V>["tag"]) => void,
): HNode<K, V> => {
  const target: HNode<K, V> = node.tag === "leaf"
    ? node
    : { tag: "branch", bitmap: node.bitmap, children: node.children.map((child) => instrumentNode(child, onVisit)) };
  return new Proxy(target, {
    get(current, prop, receiver): unknown {
      if (prop === "entries" || prop === "children") onVisit(current.tag);
      return Reflect.get(current, prop, receiver);
    },
  });
};

const instrumentTree = <K, V>(
  node: HNode<K, V> | undefined,
  onVisit: (tag: HNode<K, V>["tag"]) => void,
): HNode<K, V> | undefined => (node === undefined ? undefined : instrumentNode(node, onVisit));

test("hash maps keep their witness, collision order and prior values", () => {
  const witness = {
    eq: (a: string, b: string) => a.toLowerCase() === b.toLowerCase(),
    hash: () => 0,
  };
  const before = hashMapFromList(witness, [["A", 1], ["b", 2], ["c", 3]]);
  const after = hashMapInsert("B", 20, before);
  assert.equal(hashMapSize(after), 3);
  assert.deepEqual(hashMapGet("a", after), { _tag: "Some", value: 1 });
  assert.deepEqual(hashMapToList(after), [["A", 1], ["B", 20], ["c", 3]]);
  assert.deepEqual(hashMapToList(before), [["A", 1], ["b", 2], ["c", 3]]);
});

test("a hash set uses SameValueZero for numerical keys", () => {
  const witness = { eq: (a: number, b: number) => a === b || (Number.isNaN(a) && Number.isNaN(b)), hash: () => 0 };
  const set = hashSetFromList(witness, [0, -0, NaN, NaN]);
  assert.equal(hashSetSize(set), 2);
  assert.equal(hashSetMember(-0, set), true);
  assert.equal(hashSetMember(NaN, set), true);
});

test("empty hash collections produce empty sequences and build cleanly from empty Seq", () => {
  const emptyMap = hashMapWith<number, string>(intWitness);
  const emptySet = hashSetWith<number>(intWitness);
  assert.deepEqual(seqToList(hashMapToSeq(emptyMap)), []);
  assert.deepEqual(seqFirst(hashMapToSeq(emptyMap)), { _tag: "None" });
  assert.deepEqual(seqToList(hashSetToSeq(emptySet)), []);
  assert.deepEqual(seqFirst(hashSetToSeq(emptySet)), { _tag: "None" });

  const builtMap = hashMapFromSeq(intWitness, []);
  const builtSet = hashSetFromSeq(intWitness, []);
  assert.equal(hashMapSize(builtMap), 0);
  assert.equal(builtMap.witness, intWitness);
  assert.equal(hashSetSize(builtSet), 0);
  assert.equal(builtSet.witness, intWitness);
});

test("toSeq is restartable, supports interleaved cursors, and preserves HAMT order including bit 31 and collisions", () => {
  const map = hashMapFromSeq(
    intWitness,
    [[-1, "neg1"], [-2147483648, "min32"], [64, "64"], [32, "32"], [0, "0"], [1, "1"], [33, "33"]],
  );
  const expected: Array<[number, string]> = [
    [0, "0"],
    [-2147483648, "min32"],
    [32, "32"],
    [64, "64"],
    [1, "1"],
    [33, "33"],
    [-1, "neg1"],
  ];
  const entries = hashMapToSeq(map);
  assert.deepEqual(seqToList(seqTake(3, entries)), expected.slice(0, 3));
  assert.deepEqual(seqToList(entries), expected);

  // Interleave two independent iterators on the same Seq.
  const left = entries[Symbol.iterator]();
  const right = entries[Symbol.iterator]();
  assert.deepEqual(left.next(), { done: false, value: [0, "0"] });
  assert.deepEqual(left.next(), { done: false, value: [-2147483648, "min32"] });
  assert.deepEqual(right.next(), { done: false, value: [0, "0"] });
  assert.deepEqual(left.next(), { done: false, value: [32, "32"] });
  assert.deepEqual(right.next(), { done: false, value: [-2147483648, "min32"] });

  const mod3 = { eq: (a: number, b: number) => a === b, hash: (n: number) => n % 3 };
  const set = hashSetFromSeq(mod3, [7, 4, 1, 9, 6, 2, 5, 8, 3]);
  const elements = hashSetToSeq(set);
  assert.deepEqual(seqToList(seqTake(4, elements)), [9, 6, 3, 7]);
  assert.deepEqual(seqToList(elements), [9, 6, 3, 7, 4, 1, 2, 5, 8]);
  assert.deepEqual(hashSetToList(set), [9, 6, 3, 7, 4, 1, 2, 5, 8]);
});

test("fromSeq consumes finite input once in order with custom witness deduplication and last-value-wins", () => {
  const witness = {
    eq: (a: string, b: string) => a.toLowerCase() === b.toLowerCase(),
    hash: (s: string) => hash(s.toLowerCase()),
  };
  const mapSource = singlePass<readonly [string, number]>([["A", 1], ["b", 2], ["a", 10], ["B", 20], ["c", 3]]);
  const map = hashMapFromSeq(witness, mapSource.seq);
  assert.equal(mapSource.runs(), 1);
  assert.equal(mapSource.pulls(), 5);
  assert.equal(map.witness, witness);
  assert.equal(hashMapSize(map), 3);
  assert.deepEqual(hashMapGet("A", map), { _tag: "Some", value: 10 });
  assert.deepEqual(hashMapGet("b", map), { _tag: "Some", value: 20 });
  assert.deepEqual(hashMapGet("C", map), { _tag: "Some", value: 3 });

  const setSource = singlePass(["Ada", "ada", "Bob", "BOB", "Cid"]);
  const set = hashSetFromSeq(witness, setSource.seq);
  assert.equal(setSource.runs(), 1);
  assert.equal(setSource.pulls(), 5);
  assert.equal(set.witness, witness);
  assert.equal(hashSetSize(set), 3);
  assert.deepEqual(seqToList(hashSetToSeq(set)).toSorted(), ["Ada", "Bob", "Cid"]);
});

test("persistent snapshots stay isolated from later mutations after owned construction", () => {
  const map0 = hashMapFromSeq(intWitness, [[0, "zero"], [32, "thirty-two"], [1, "one"], [2, "two"]]);
  const mapSeq0 = hashMapToSeq(map0);
  assert.deepEqual(seqToList(seqTake(2, mapSeq0)), [[0, "zero"], [32, "thirty-two"]]);

  const map1 = hashMapInsert(33, "thirty-three", hashMapInsert(0, "ZERO", hashMapRemove(32, map0)));
  assert.deepEqual(seqToList(mapSeq0), [[0, "zero"], [32, "thirty-two"], [1, "one"], [2, "two"]]);
  assert.deepEqual(seqToList(hashMapToSeq(map1)), [[0, "ZERO"], [1, "one"], [33, "thirty-three"], [2, "two"]]);

  const set0 = hashSetFromSeq(intWitness, [0, 32, 1, 2]);
  const setSeq0 = hashSetToSeq(set0);
  assert.deepEqual(seqToList(seqTake(2, setSeq0)), [0, 32]);

  const set1 = hashSetInsert(33, hashSetRemove(32, set0));
  assert.deepEqual(seqToList(setSeq0), [0, 32, 1, 2]);
  assert.deepEqual(seqToList(hashSetToSeq(set1)), [0, 1, 33, 2]);
});

test("taking a prefix visits only the HAMT nodes needed rather than traversing the full collection", () => {
  const size = 4096;
  const mapPairs: Array<readonly [number, number]> = Array.from({ length: size }, (_, i) => [i, i * 2]);
  const baseMap = hashMapFromSeq(intWitness, mapPairs);
  const baseSet = hashSetFromSeq(intWitness, seqRange(0, size));

  let mapVisits = 0;
  const instrumentedMap = new FlickHashMap(
    baseMap.witness,
    instrumentTree(baseMap.root, () => { mapVisits++; }),
    baseMap.size,
  );
  const mapSeq = hashMapToSeq(instrumentedMap);

  assert.deepEqual(seqToList(seqTake(0, mapSeq)), []);
  assert.equal(mapVisits, 0, "take 0 must not visit any HAMT nodes");

  const mapPrefix = seqToList(seqTake(3, mapSeq));
  assert.deepEqual(mapPrefix, [[0, 0], [1024, 2048], [2048, 4096]]);
  assert.ok(mapVisits > 0 && mapVisits <= 20, `expected bounded prefix visits, got ${mapVisits}`);

  const prefixVisits = mapVisits;
  assert.deepEqual(seqFirst(mapSeq), { _tag: "Some", value: [0, 0] });
  assert.ok(mapVisits - prefixVisits <= 10, `expected single-path visits for first, got ${mapVisits - prefixVisits}`);

  let setVisits = 0;
  const instrumentedSet = new FlickHashSet(
    new FlickHashMap(
      baseSet.map.witness,
      instrumentTree(baseSet.map.root, () => { setVisits++; }),
      baseSet.map.size,
    ),
  );
  const setSeq = hashSetToSeq(instrumentedSet);

  assert.deepEqual(seqToList(seqTake(0, setSeq)), []);
  assert.equal(setVisits, 0, "take 0 must not visit any HAMT nodes");

  const setPrefix = seqToList(seqTake(3, setSeq));
  assert.deepEqual(setPrefix, [0, 1024, 2048]);
  assert.ok(setVisits > 0 && setVisits <= 20, `expected bounded prefix visits, got ${setVisits}`);
});
