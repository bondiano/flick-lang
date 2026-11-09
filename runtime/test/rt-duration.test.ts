import assert from "node:assert/strict";
import { test } from "node:test";
import { equals, hash } from "../src/core/data.ts";
import { durationCompare, durationFormat, durationMillis, durationNanos } from "../src/rt/duration.ts";

test("millisecond and nanosecond durations have equal values and hashes", () => {
  const millis = durationMillis(5);
  const nanos = durationNanos(5_000_000n);
  assert.equal(millis._tag, "Success");
  assert.equal(nanos._tag, "Success");
  if (millis._tag !== "Success" || nanos._tag !== "Success") return;
  assert.equal(equals(millis.success, nanos.success), true);
  assert.equal(hash(millis.success), hash(nanos.success));
  assert.equal(durationFormat(nanos.success), "5ms");
});

test("nanosecond durations remain distinct beyond millisecond precision", () => {
  const first = durationNanos(1_000_000_000_000_000_000n);
  const second = durationNanos(1_000_000_000_000_000_001n);
  assert.equal(first._tag, "Success");
  assert.equal(second._tag, "Success");
  if (first._tag !== "Success" || second._tag !== "Success") return;
  assert.equal(equals(first.success, second.success), false);
  assert.notEqual(durationCompare(first.success, second.success)._tag, durationCompare(second.success, first.success)._tag);
});
