// The packed-bytes bindings give the adapter's options and results, which
// `==` compares with those Flick's constructors build on Effect.
import assert from "node:assert/strict";
import { test } from "node:test";

import { Option, Result } from "effect";

import {
  bytesDecodeBase64Url,
  bytesDecodeUtf8,
  bytesEncodeUtf8,
  bytesFromList,
  bytesGet,
  bytesSlice,
  bytesToList,
  equals,
} from "../src/index.ts";

test("the options of bytes equal Effect's", () => {
  const bytes = bytesEncodeUtf8("я");
  assert.ok(equals(bytesDecodeUtf8(bytes), Option.some("я")));
  assert.ok(equals(bytesGet(1, bytes), Option.some(143)));
  assert.ok(equals(bytesGet(2, bytes), Option.none()));
  assert.ok(equals(bytesSlice(1, 3, bytes), Option.none()));
  assert.deepEqual(bytesToList(Option.getOrThrow(bytesSlice(1, 2, bytes))), [143]);
  assert.ok(equals(bytesDecodeBase64Url("!"), Option.none()));
  assert.deepEqual(bytesToList(Option.getOrThrow(bytesDecodeBase64Url("AID_"))), [0, 128, 255]);
});

test("invalid UTF-8 is None", () => {
  const invalid = Result.getOrThrow(bytesFromList([237, 160, 128]));
  assert.ok(equals(bytesDecodeUtf8(invalid), Option.none()));
});

test("the results of bytes equal Effect's", () => {
  assert.ok(equals(bytesFromList([256, -1]), Result.fail([0, 256])));
  const failed = bytesFromList([1, 256]);
  assert.ok(Result.isFailure(failed));
  assert.deepEqual(failed.failure, [1, 256]);
  assert.deepEqual(bytesToList(Result.getOrThrow(bytesFromList([7]))), [7]);
});
