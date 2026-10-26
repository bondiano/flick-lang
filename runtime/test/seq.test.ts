// A `Seq` is a value: every iteration starts over, and `take` pulls no
// element past the last one it keeps, so infinite sequences end under it.
import assert from "node:assert/strict";
import { test } from "node:test";

import { listRange } from "../src/core/list.ts";
import { seq, seqFrom, seqMap, seqRangeBy, seqRangeTo, seqTake, seqToList } from "../src/core/seq.ts";

test("a sequence iterates afresh each time", () => {
  const squares = seqTake(3, seqMap((n: number) => n * n, seqFrom(1)));
  assert.deepEqual(seqToList(squares), [1, 4, 9]);
  assert.deepEqual(seqToList(squares), [1, 4, 9]);
});

test("take pulls nothing after its last element", () => {
  let pulled = 0;
  const counted = seq(function* () {
    for (let i = 0; ; i++) {
      pulled++;
      yield i;
    }
  });
  assert.deepEqual(seqToList(seqTake(2, counted)), [0, 1]);
  assert.equal(pulled, 2);
});

test("a range ends where a step no longer changes the number", () => {
  const big = 2 ** 53;
  assert.deepEqual(seqToList(seqTake(3, seqRangeTo(big, big))), [big]);
  assert.deepEqual(seqToList(seqTake(3, seqRangeTo(big, big + 2))), [big]);
  assert.deepEqual(seqToList(seqTake(3, seqRangeBy(big, big + 4, 1))), [big]);
  assert.deepEqual(seqToList(seqTake(3, seqFrom(big))), [big]);
  assert.deepEqual(listRange(big, big + 2), [big]);
  assert.deepEqual(seqToList(seqRangeTo(7, 7)), [7]);
  assert.deepEqual(seqToList(seqRangeBy(5, 0, -2)), [5, 3, 1]);
  assert.deepEqual(seqToList(seqRangeBy(0, 5, 0)), []);
});
