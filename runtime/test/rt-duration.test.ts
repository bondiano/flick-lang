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

test("a zero duration is written 0, as Effect's and the go runtime's format write it", () => {
  const zero = durationMillis(0);
  const nanos = durationNanos(0n);
  assert.equal(zero._tag, "Success");
  assert.equal(nanos._tag, "Success");
  if (zero._tag !== "Success" || nanos._tag !== "Success") return;
  assert.equal(durationFormat(zero.success), "0");
  assert.equal(durationFormat(nanos.success), "0");
});

test("a duration of nanoseconds too many for a number is finite, its days written as Effect writes them", () => {
  const huge = durationNanos(10n ** 400n + 38_296_453_340_842n);
  assert.equal(huge._tag, "Success");
  if (huge._tag !== "Success") return;
  // what Effect 4.1.2's `Duration.format` gives for the same nanoseconds
  assert.equal(durationFormat(huge.success), "Infinityd 4h 24m 56s 453ms 340842ns");
});
