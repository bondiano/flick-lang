import assert from "node:assert/strict";
import { test } from "node:test";

import { ChunkBranch, type Chunk } from "../src/core/chunk.ts";
import { copyData, equals, hash } from "../src/core/data.ts";
import { chunkAppend, chunkConcat, chunkEmpty, chunkFromList, chunkGet, chunkSize, chunkToList } from "../src/rt/chunk.ts";
import { appendAll, findLast, findLastIndex, last, prepend, reduce, reduceRight, splitAt } from "../src/rt/effect/Chunk.ts";
import { flickDiff, flickPatch } from "../src/rt/effect/Differ.ts";
import { format } from "../src/rt/effect/Formatter.ts";
import { flickToStringUnknown } from "../src/rt/effect/Inspectable.ts";
import { flickChunk, flickChunkJson, toDifferJsonPatch } from "../src/rt/effect/Schema.ts";
import { schemaInt } from "../src/rt/schema.ts";

test("appends and asymmetric concatenations share large operands and keep prior values", () => {
  const input = Array.from({ length: 4096 }, (_, index) => index);
  const snapshot = chunkFromList(input);
  const branch = chunkAppend(4096, snapshot);
  assert.ok(branch instanceof ChunkBranch);
  assert.equal(branch.left, snapshot, "a large append must reuse its prefix");

  const appended = appendAll(chunkFromList([4097, 4098]))(branch);
  const prepended = prepend(-1)(appended);
  const doubled = chunkConcat(prepended, prepended);
  assert.ok(doubled instanceof ChunkBranch);
  assert.equal(doubled.left, prepended);
  assert.equal(doubled.right, prepended);
  input[0] = 10;
  const copy = chunkToList(doubled);
  copy[0] = 20;
  assert.equal(chunkSize(doubled), 8200);
  assert.deepEqual(chunkGet(0, doubled), { _tag: "Some", value: -1 });
  assert.deepEqual(chunkGet(4100, doubled), { _tag: "Some", value: -1 });
  assert.deepEqual(chunkGet(8199, doubled), { _tag: "Some", value: 4098 });
  assert.deepEqual(chunkGet(8200, doubled), { _tag: "None" });
  assert.deepEqual(chunkGet(0.5, doubled), { _tag: "None" });
  assert.deepEqual(chunkGet(-1, doubled), { _tag: "None" });
  assert.deepEqual(chunkGet(0, snapshot), { _tag: "Some", value: 0 });
  assert.equal(chunkSize(snapshot), 4096);
});

test("repeated joins preserve order, structural equality and hashes across shapes", () => {
  let right: Chunk<number> = chunkEmpty;
  let left: Chunk<number> = chunkEmpty;
  const size = 8192;
  for (let i = 0; i < size; i++) {
    right = chunkAppend(i, right);
    left = chunkConcat(chunkFromList([size - i - 1]), left);
  }
  const checkBalance = (chunk: Chunk<number>): number => {
    if (!(chunk instanceof ChunkBranch)) return 0;
    const leftHeight = checkBalance(chunk.left);
    const rightHeight = checkBalance(chunk.right);
    assert.ok(Math.abs(leftHeight - rightHeight) <= 1);
    assert.equal(chunk.length, chunk.left.length + chunk.right.length);
    assert.equal(chunk.height, 1 + Math.max(leftHeight, rightHeight));
    return chunk.height;
  };
  assert.ok(checkBalance(left) > 0);
  assert.ok(checkBalance(right) > 0);
  const items = Array.from({ length: size }, (_, index) => index);
  const flat = chunkFromList(items);
  assert.ok(equals(right, left));
  assert.ok(equals(flat, right));
  assert.ok(equals(right, flat));
  assert.equal(hash(right), hash(flat));
  assert.equal(hash(left), hash(flat));
  assert.deepEqual(chunkToList(left), items);
  assert.deepEqual(reduce(0, (a, b: number) => a + b)(right), size * (size - 1) / 2);
  assert.deepEqual(reduceRight("", (a, b: number) => a + String(b))(right), items.toReversed().join(""));
  const [before, after] = splitAt(333)(right);
  assert.deepEqual(chunkToList(before), items.slice(0, 333));
  assert.deepEqual(chunkToList(after), items.slice(333));
});

test("chunk schemas validate the original chunk and read and write JSON arrays", () => {
  const items = Array.from({ length: 64 }, (_, index) => index);
  const branch = chunkAppend(64, chunkFromList(items));
  for (const schema of [flickChunk(schemaInt), flickChunkJson(schemaInt)]) {
    const wire = [...items, 64];
    assert.deepEqual(schema.encode(branch), { _tag: "Success", success: wire });
    assert.deepEqual(schema.decode(wire), { _tag: "Success", success: wire });
    const validated = schema.value(branch, "");
    assert.equal(validated._tag, "Success");
    if (validated._tag === "Success") assert.equal(validated.success, branch);
    const invalid = schema.value(chunkAppend("invalid", chunkFromList<unknown>(items)), "chunk");
    assert.equal(invalid._tag, "Failure");
    if (invalid._tag === "Failure") assert.equal(invalid.failure._0[0]?.path, "chunk[64]");
  }
});

test("foreign copying still receives an isolated flat sequence", () => {
  const record = { value: 7 };
  const prefix = chunkFromList(Array.from({ length: 64 }, () => record));
  const chunk = chunkAppend(record, prefix);
  const copied = copyData(chunk);
  assert.ok(Array.isArray(copied));
  assert.ok(equals(copied, chunk));
  const first: unknown = copied[0];
  assert.ok(typeof first === "object" && first !== null && "value" in first && first !== record);
  first.value = 21;
  assert.equal(record.value, 7);
});

test("generic formatting preserves the flat representation of a nested chunk", () => {
  const items = Array.from({ length: 64 }, (_, index) => index);
  const chunk = chunkAppend(64, chunkFromList(items));
  const expected = { contents: [...items, 64] };
  assert.equal(format({ contents: chunk }), JSON.stringify(expected));
  assert.equal(flickToStringUnknown(2, { contents: chunk }), JSON.stringify(expected, null, 2));
});

test("a JSON differ applies a patch to the encoded chunk contents", () => {
  const items = Array.from({ length: 64 }, (_, index) => index);
  const first = chunkAppend(64, chunkFromList(items));
  const second = chunkAppend(65, first);
  const differ = toDifferJsonPatch(flickChunkJson(schemaInt));
  const patch = flickDiff(first, second, differ);
  assert.deepEqual(patch, [{ op: "add", path: "/65", value: 65 }]);
  assert.ok(equals(flickPatch(first, patch, differ), second));
});

test("shared concatenation rejects an inexact size without materializing it", () => {
  let chunk = chunkFromList([7]);
  for (let i = 0; i < 31; i++) chunk = chunkConcat(chunk, chunk);
  assert.equal(chunkSize(chunk), -2147483648);
  for (let i = 31; i < 52; i++) chunk = chunkConcat(chunk, chunk);
  assert.equal(chunk.length, 2 ** 52);
  assert.deepEqual(last(chunk), { _tag: "Some", value: 7 });
  assert.deepEqual(findLast(() => true)(chunk), { _tag: "Some", value: 7 });
  assert.deepEqual(findLastIndex(() => true)(chunk), { _tag: "Some", value: -1 });
  assert.throws(() => chunkConcat(chunk, chunk), RangeError);
});
