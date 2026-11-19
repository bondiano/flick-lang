import assert from "node:assert/strict";
import { test } from "node:test";

import { Effect, Equal, Option } from "effect";
import { some } from "@flick-lang/runtime/core/data";
import { chunkAppend, chunkFromList, chunkGet, chunkToList } from "@flick-lang/runtime/rt/chunk";
import * as Native from "@flick-lang/runtime/rt/collections";

import { chunkFromEffect, chunkToEffect, hashMapFromEffect, hashMapToEffect, hashSetFromEffect, hashSetToEffect } from "../src/bridge_collections.ts";
import { dataFromEffect, dataToEffect } from "../src/bridge_data.ts";
import { fromChunk, runCollect } from "../src/effect/Channel.ts";
import * as TxChunk from "../src/effect/TxChunk.ts";
import * as TxHashMap from "../src/effect/TxHashMap.ts";
import * as TxHashSet from "../src/effect/TxHashSet.ts";
import * as Host from "../src/stdlib/list.ts";

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
  const values = Array.from({ length: 64 }, (_, index) => index);
  const source = chunkAppend(64, chunkFromList(values));
  const host = chunkToEffect(source);
  const tx = await Effect.runPromise(TxChunk.make(host));
  await Effect.runPromise(TxChunk.append(tx, 65));
  const after = chunkFromEffect(await Effect.runPromise(TxChunk.get(tx)));
  assert.deepEqual(chunkToList(after), [...values, 64, 65]);
  assert.deepEqual(chunkGet(64, source), { _tag: "Some", value: 64 });
  assert.deepEqual(chunkToList(source), [...values, 64]);
  assert.deepEqual(await Effect.runPromise(runCollect(fromChunk(host))), [...values, 64]);
  const channel = fromChunk(source);
  const collected = await Promise.all([
    Effect.runPromise(runCollect(channel)),
    Effect.runPromise(runCollect(channel)),
  ]);
  assert.deepEqual(collected, [[...values, 64], [...values, 64]]);
});

test("the default Effect adapter keeps append and concat on a persistent Chunk", () => {
  const size = 8192;
  const values = Array.from({ length: size }, (_, index) => index);
  let appended: ReturnType<typeof Host.chunkFromList<number>> = Host.chunkEmpty;
  let concatenated = appended;
  for (let index = 0; index < size; index++) {
    appended = Host.chunkAppend(index, appended);
    concatenated = Host.chunkConcat(Host.chunkFromList([size - index - 1]), concatenated);
  }
  assert.ok(Equal.equals(appended, concatenated));
  assert.ok(Equal.equals(Host.chunkFromList(values), appended));
  assert.deepEqual(Host.chunkToList(appended), values);
  assert.deepEqual(Host.chunkToList(concatenated), values);
});

test("Effect chunk sizes follow the Flick Int representation", () => {
  let chunk = Host.chunkFromList([7]);
  for (let i = 0; i < 31; i++) chunk = Host.chunkConcat(chunk, chunk);
  assert.equal(Host.chunkSize(chunk), -2147483648);
});

test("generic service boundaries convert elements in a native chunk", () => {
  const numbers = Array.from({ length: 64 }, (_, index) => some(index));
  const chunk = chunkAppend(some(64), chunkFromList(numbers));
  const host = dataToEffect(chunk);
  assert.ok(Array.isArray(host) && host.every(Option.isOption));
  assert.deepEqual(dataFromEffect(host), [...numbers, some(64)]);
});
