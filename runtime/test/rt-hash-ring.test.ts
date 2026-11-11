import assert from "node:assert/strict";
import { test } from "node:test";
import { combine, optimize, string } from "../src/rt/effect/Hash.ts";

test("pure hashes keep their stable numeric values", () => {
  assert.equal(string("hello"), 181380007);
  assert.equal(string("world"), 164394279);
  assert.equal(combine(23)(17), -42410853);
  assert.equal(optimize(-1), -1);
});
