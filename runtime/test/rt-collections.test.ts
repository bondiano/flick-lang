import assert from "node:assert/strict";
import { test } from "node:test";

import {
  hashMapFromList,
  hashMapGet,
  hashMapInsert,
  hashMapSize,
  hashMapToList,
  hashSetFromList,
  hashSetMember,
  hashSetSize,
} from "../src/rt/collections.ts";

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
