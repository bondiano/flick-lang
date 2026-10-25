// A `Seq` is a value: every iteration starts over, and `take` pulls no
// element past the last one it keeps, so infinite sequences end under it.
import assert from "node:assert/strict";
import { test } from "node:test";

import { seq, seqFrom, seqMap, seqTake, seqToList } from "../src/core/seq.ts";

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
