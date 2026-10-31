// Flick data crosses a binding outside the runtime as a copy
// (docs/formal/ts-holes.md Р26): JS that changes what it got or gave in
// place leaves the Flick value as it was.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Option, Result } from "effect";

import { copyData as coreCopy, none, some } from "../src/core/data.ts";
import { copyData } from "../src/data.ts";

test("arrays and plain objects are copied all the way down", () => {
  const user = { name: "ann", tags: ["a"], home: { _tag: "Some", value: [1] } };
  const copy = coreCopy(user);
  assert.deepEqual(copy, user);
  copy.tags.push("b");
  copy.home.value.push(2);
  assert.deepEqual(user, { name: "ann", tags: ["a"], home: { _tag: "Some", value: [1] } });
});

test("handles, functions and frozen constants keep their identity", () => {
  const date = new Date(0);
  const f = () => 1;
  const [d, g, n] = coreCopy([date, f, none]);
  assert.equal(d, date);
  assert.equal(g, f);
  assert.equal(n, none);
  assert.deepEqual(coreCopy(some([1])), some([1]));
});

test("on Effect, Option and Result are rebuilt around copies", () => {
  const inner = [1];
  const option = copyData(Option.some(inner));
  assert.ok(Option.isSome(option));
  option.value.push(2);
  assert.deepEqual(inner, [1]);
  const result = copyData(Result.fail([3]));
  assert.ok(Result.isFailure(result));
  assert.deepEqual(result.failure, [3]);
  assert.equal(copyData(Option.none()), Option.none());
});
