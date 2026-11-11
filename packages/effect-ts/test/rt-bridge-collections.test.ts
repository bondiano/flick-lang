import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect } from "effect";
import * as Native from "@flick-lang/runtime/rt/collections";

import { chunkFromEffect, chunkToEffect, hashMapFromEffect, hashMapToEffect, hashSetFromEffect, hashSetToEffect } from "../src/bridge_collections.ts";
import * as TxChunk from "../src/effect/TxChunk.ts";
import * as TxHashMap from "../src/effect/TxHashMap.ts";
import * as TxHashSet from "../src/effect/TxHashSet.ts";

const witness: Native.Witness<string> = {
  eq: (a, b) => a.toLowerCase() === b.toLowerCase(),
  hash: () => 1,
};

test("a transactional map snapshot keeps its witness and remains immutable on the native side", async () => {
  const map = await Effect.runPromise(TxHashMap.flickMake(witness, [["Ada", 1]]));
  const snapshot = hashMapFromEffect(await Effect.runPromise(TxHashMap.flickSnapshot(map)));
  assert.equal(snapshot.witness, witness);
  assert.deepEqual(Native.hashMapGet("ADA", snapshot), { _tag: "Some", value: 1 });
  const changed = hashMapToEffect(Native.hashMapInsert("ada", 2, snapshot));
  await Effect.runPromise(TxHashMap.flickUnion(changed, map));
  assert.deepEqual(Native.hashMapToList(hashMapFromEffect(await Effect.runPromise(TxHashMap.flickSnapshot(map)))), [["ada", 2]]);
  assert.deepEqual(Native.hashMapToList(snapshot), [["Ada", 1]]);
});

test("a native set passes through a transaction without losing custom equality", async () => {
  const set = Native.hashSetFromList(witness, ["Ada", "ada", "Bob"]);
  const tx = await Effect.runPromise(TxHashSet.flickFromHashSet(hashSetToEffect(set)));
  const snapshot = hashSetFromEffect(await Effect.runPromise(TxHashSet.flickToHashSet(tx)));
  assert.equal(snapshot.witness, witness);
  assert.equal(Native.hashSetSize(snapshot), 2);
  assert.equal(Native.hashSetMember("BOB", snapshot), true);
});

test("native chunks cross transactional reads and writes as persistent values", async () => {
  const source = [1, 2];
  const tx = await Effect.runPromise(TxChunk.make(chunkToEffect(source)));
  await Effect.runPromise(TxChunk.append(tx, 3));
  assert.deepEqual(chunkFromEffect(await Effect.runPromise(TxChunk.get(tx))), [1, 2, 3]);
  assert.deepEqual(source, [1, 2]);
});
