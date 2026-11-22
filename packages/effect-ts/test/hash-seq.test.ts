import assert from "node:assert/strict";
import { test } from "node:test";

import { seq, seqFirst, seqRange, seqTake, seqToList, type Seq } from "@flick-lang/runtime/core/seq";
import { hash } from "@flick-lang/runtime/rt/hash";

import { some } from "../src/data.ts";
import {
  hashMapFromSeq,
  hashMapGet,
  hashMapInsert,
  hashMapRemove,
  hashMapSize,
  hashMapToList,
  hashMapToSeq,
  hashMapWith,
  hashSetFromSeq,
  hashSetInsert,
  hashSetMember,
  hashSetRemove,
  hashSetSize,
  hashSetToList,
  hashSetToSeq,
  hashSetWith,
} from "../src/stdlib/hash.ts";

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

test("Effect adapter hashMapToSeq and hashSetToSeq handle empty, repeat, partial, collisions, and snapshots", () => {
  assert.deepEqual(seqToList(hashMapToSeq(hashMapWith<number, string>(intWitness))), []);
  assert.deepEqual(seqToList(hashSetToSeq(hashSetWith<number>(intWitness))), []);

  const map0 = hashMapFromSeq(intWitness, [[64, "64"], [32, "32"], [0, "0"], [1, "1"], [33, "33"]]);
  const mapSeq0 = hashMapToSeq(map0);
  const expected: Array<[number, string]> = [[0, "0"], [32, "32"], [64, "64"], [1, "1"], [33, "33"]];
  assert.deepEqual(seqToList(seqTake(3, mapSeq0)), expected.slice(0, 3));
  assert.deepEqual(seqToList(mapSeq0), expected);

  const map1 = hashMapInsert(2, "2", hashMapRemove(32, map0));
  assert.deepEqual(seqToList(mapSeq0), expected);
  assert.deepEqual(seqToList(hashMapToSeq(map1)), [[0, "0"], [64, "64"], [1, "1"], [33, "33"], [2, "2"]]);

  const mod3 = { eq: (a: number, b: number) => a === b, hash: (n: number) => n % 3 };
  const set0 = hashSetFromSeq(mod3, [7, 4, 1, 9, 6, 2, 5, 8, 3]);
  const setSeq0 = hashSetToSeq(set0);
  assert.deepEqual(seqToList(seqTake(4, setSeq0)), [9, 6, 3, 7]);
  assert.deepEqual(seqToList(setSeq0), [9, 6, 3, 7, 4, 1, 2, 5, 8]);

  const set1 = hashSetInsert(12, hashSetRemove(6, set0));
  assert.deepEqual(seqToList(setSeq0), [9, 6, 3, 7, 4, 1, 2, 5, 8]);
  assert.deepEqual(hashSetToList(set1), [9, 3, 12, 7, 4, 1, 2, 5, 8]);
});

test("Effect adapter hashMapFromSeq and hashSetFromSeq consume finite input once under custom witnesses", () => {
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
  assert.deepEqual(hashMapGet("A", map), some(10));
  assert.deepEqual(hashMapToList(map).toSorted(([a], [b]) => a.localeCompare(b)), [["a", 10], ["B", 20], ["c", 3]]);

  const setSource = singlePass(["Ada", "ada", "Bob", "BOB", "Cid"]);
  const set = hashSetFromSeq(witness, setSource.seq);
  assert.equal(setSource.runs(), 1);
  assert.equal(setSource.pulls(), 5);
  assert.equal(set.witness, witness);
  assert.equal(hashSetSize(set), 3);
  assert.equal(hashSetMember("BOB", set), true);
  assert.deepEqual(seqToList(hashSetToSeq(set)).toSorted(), ["Ada", "Bob", "Cid"]);
});

// Verify lazy iteration stops before touching non-prefix root children in Effect's HAMT.
const instrumentRoot = (nativeMap: object): { readonly touched: Set<number> } => {
  const root: unknown = Reflect.get(nativeMap, "_root");
  assert.ok(typeof root === "object" && root !== null);
  const children: unknown = Reflect.get(root, "children");
  assert.ok(Array.isArray(children) && children.length >= 16);
  const touched = new Set<number>();
  Reflect.set(
    root,
    "children",
    new Proxy(children, {
      get(target, prop, receiver): unknown {
        if (typeof prop === "string" && /^\d+$/.test(prop)) {
          const index = Number(prop);
          const child: unknown = Reflect.get(target, prop, receiver);
          if (typeof child === "object" && child !== null) {
            return new Proxy(child, {
              get(childTarget, childProp, childReceiver): unknown {
                if (childProp === "_tag") touched.add(index);
                return Reflect.get(childTarget, childProp, childReceiver);
              },
            });
          }
          return child;
        }
        return Reflect.get(target, prop, receiver);
      },
    }),
  );
  return { touched };
};

test("Effect adapter prefix traversal does not visit untouched root subtrees", () => {
  const size = 4096;
  const map = hashMapFromSeq(intWitness, seq(function* () {
    for (let i = 0; i < size; i++) yield [i, i * 2] as const;
  }));
  const set = hashSetFromSeq(intWitness, seqRange(0, size));

  const mapProbe = instrumentRoot(map.native);
  const mapSeq = hashMapToSeq(map);
  assert.deepEqual(seqToList(seqTake(0, mapSeq)), []);
  assert.equal(mapProbe.touched.size, 0, "take 0 must not visit any root child");

  const mapPrefix = seqToList(seqTake(3, mapSeq));
  assert.deepEqual(mapPrefix, [[0, 0], [1024, 2048], [2048, 4096]]);
  assert.ok(mapProbe.touched.size > 0 && mapProbe.touched.size <= 2, `expected <= 2 touched root subtrees, got ${mapProbe.touched.size}`);
  assert.deepEqual(seqFirst(mapSeq), { _tag: "Some", value: [0, 0] });

  const setKeyMap: unknown = Reflect.get(set.native, "keyMap");
  assert.ok(typeof setKeyMap === "object" && setKeyMap !== null);
  const setProbe = instrumentRoot(setKeyMap);
  const setSeq = hashSetToSeq(set);
  assert.deepEqual(seqToList(seqTake(0, setSeq)), []);
  assert.equal(setProbe.touched.size, 0, "take 0 must not visit any root child");

  const setPrefix = seqToList(seqTake(3, setSeq));
  assert.deepEqual(setPrefix, [0, 1024, 2048]);
  assert.ok(setProbe.touched.size > 0 && setProbe.touched.size <= 2, `expected <= 2 touched root subtrees, got ${setProbe.touched.size}`);
});
