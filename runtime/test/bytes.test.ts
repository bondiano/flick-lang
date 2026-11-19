import assert from "node:assert/strict";
import { test } from "node:test";

import {
  bytesDecodeBase64Url,
  bytesDecodeUtf8,
  bytesEmpty,
  bytesEncodeBase64Url,
  bytesEncodeUtf8,
  bytesEqualConstantTime,
  bytesFromList,
  bytesGet,
  bytesLength,
  bytesSlice,
  bytesToList,
} from "../src/core/bytes.ts";

const fromList = (list: ReadonlyArray<number>) => {
  const result = bytesFromList(list);
  assert.equal(result._tag, "Success");
  return result.success;
};

test("packed bytes copy both directions of list conversion", () => {
  const input = [0, 127, 255];
  const packed = fromList(input);
  const slice = bytesSlice(1, 3, packed);
  assert.equal(slice._tag, "Some");

  input[1] = 3;
  const copy = bytesToList(packed);
  assert.deepEqual(copy, [0, 127, 255]);
  copy[1] = 4;
  const sliceCopy = bytesToList(slice.value);
  assert.deepEqual(sliceCopy, [127, 255]);
  sliceCopy[0] = 5;

  assert.deepEqual(bytesToList(packed), [0, 127, 255]);
  assert.deepEqual(bytesToList(slice.value), [127, 255]);
  assert.deepEqual(bytesFromList([0, 256, -1]), { _tag: "Failure", failure: [1, 256] });
  assert.deepEqual(bytesFromList([-1, 256]), { _tag: "Failure", failure: [0, -1] });
});

test("packed bytes check indexes and half-open slices", () => {
  const packed = fromList([0, 255]);
  assert.equal(bytesLength(bytesEmpty), 0);
  assert.deepEqual(bytesToList(fromList([])), []);
  assert.equal(bytesLength(packed), 2);
  assert.deepEqual(bytesGet(0, packed), { _tag: "Some", value: 0 });
  assert.deepEqual(bytesGet(1, packed), { _tag: "Some", value: 255 });
  assert.deepEqual(bytesGet(-1, packed), { _tag: "None" });
  assert.deepEqual(bytesGet(2, packed), { _tag: "None" });
  assert.deepEqual(bytesGet(0, bytesEmpty), { _tag: "None" });
  assert.deepEqual(bytesSlice(-1, 0, packed), { _tag: "None" });
  assert.deepEqual(bytesSlice(1, 0, packed), { _tag: "None" });
  assert.deepEqual(bytesSlice(2, 3, packed), { _tag: "None" });
  assert.deepEqual(bytesSlice(3, 3, packed), { _tag: "None" });
  assert.deepEqual(bytesSlice(2, 2, packed), { _tag: "Some", value: bytesEmpty });
});

test("packed bytes encode and decode UTF-8 without stripping the BOM", () => {
  assert.deepEqual(bytesToList(bytesEncodeUtf8("\uFEFFé😀")), [239, 187, 191, 195, 169, 240, 159, 152, 128]);
  assert.deepEqual(bytesToList(bytesEncodeUtf8("\uD800")), [239, 191, 189]);
  assert.deepEqual(bytesDecodeUtf8(fromList([239, 187, 191, 65])), { _tag: "Some", value: "\uFEFFA" });
  assert.deepEqual(bytesDecodeUtf8(fromList([195])), { _tag: "None" });
});

test("packed base64url uses the URL alphabet and validates padding", () => {
  assert.equal(bytesEncodeBase64Url(fromList([251, 255])), "-_8");
  for (const input of ["-_8", "-_8="]) {
    const result = bytesDecodeBase64Url(input);
    assert.equal(result._tag, "Some");
    assert.deepEqual(bytesToList(result.value), [251, 255]);
  }
  for (const input of ["-", "+/8", "-_8==", "Zg=", "-_8\n"]) {
    assert.deepEqual(bytesDecodeBase64Url(input), { _tag: "None" }, input);
  }
  assert.deepEqual(bytesDecodeBase64Url(""), { _tag: "Some", value: bytesEmpty });
  const trailingBits = bytesDecodeBase64Url("Zh");
  assert.equal(trailingBits._tag, "Some");
  assert.deepEqual(bytesToList(trailingBits.value), [102]);
});

test("packed base64url encodes beyond the host call argument limit", () => {
  const list = Array.from({ length: 200_000 }, (_, index) => index % 256);
  assert.equal(bytesEncodeBase64Url(fromList(list)), Buffer.from(list).toString("base64url"));
});

test("packed bytes compare every same-length input by content", () => {
  const packed = fromList([1, 2, 3]);
  assert.equal(bytesEqualConstantTime(bytesEmpty, bytesEmpty), true);
  assert.equal(bytesEqualConstantTime(packed, fromList([1, 2, 3])), true);
  assert.equal(bytesEqualConstantTime(packed, fromList([0, 2, 3])), false);
  assert.equal(bytesEqualConstantTime(packed, fromList([1, 2, 0])), false);
  assert.equal(bytesEqualConstantTime(packed, fromList([1, 2])), false);
});
