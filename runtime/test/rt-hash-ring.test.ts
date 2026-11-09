import assert from "node:assert/strict";
import { test } from "node:test";
import { combine, optimize, string } from "../src/rt/effect/Hash.ts";
import { flickAdd, flickGet, flickHas, flickMake, flickRemove } from "../src/rt/effect/HashRing.ts";
import { none } from "../src/core/data.ts";

test("pure hashes preserve Effect's stable numeric values", () => {
  assert.equal(string("hello"), 181380007);
  assert.equal(string("world"), 164394279);
  assert.equal(combine(23)(17), -42410853);
  assert.equal(optimize(-1), -1);
});

test("hash ring updates are persistent and node identity uses the key", () => {
  const empty = flickMake({ baseWeight: none }, (node: { name: string; version: number }) => node.name);
  const a = flickAdd({ weight: none }, { name: "a", version: 1 }, empty);
  const b = flickAdd({ weight: none }, { name: "b", version: 1 }, a);
  const changed = flickAdd({ weight: none }, { name: "a", version: 2 }, b);
  assert.deepEqual(flickGet("x", empty), none);
  assert.equal(flickHas({ name: "a", version: 9 }, a), true);
  assert.equal(flickHas({ name: "b", version: 9 }, a), false);
  assert.equal(flickHas({ name: "a", version: 9 }, flickRemove({ name: "a", version: 0 }, b)), false);
  assert.equal(flickHas({ name: "a", version: 0 }, b), true);
  assert.equal(changed.nodes.get("a")?.[0].version, 2);
  assert.equal(b.nodes.get("a")?.[0].version, 1);
});
