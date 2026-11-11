import assert from "node:assert/strict";
import { test } from "node:test";

import { peakRss } from "./index.mjs";
import { compare, wasmBudget } from "./ratchet.mjs";

const time = (budget, extra = {}) => ({ x: { better: "lower", unit: "ms", budget, tolerance: 0.25, ...extra } });
const rate = (budget, extra = {}) => ({ x: { better: "higher", unit: "req/s", budget, tolerance: 0.25, ...extra } });
const errors = (base, head) => compare(base, head).flatMap((c) => (c.error ? [c.error] : []));

test("a tighter budget, or a first one, passes", () => {
  assert.deepEqual(compare(time(100), time(90)), [{ line: "x: 100 ms ±25% -> 90 ms ±25%, tighter" }]);
  assert.deepEqual(errors(time(null), time(90)), []);
  assert.deepEqual(errors(rate(100), rate(120)), []);
  assert.deepEqual(errors(time(100), time(100, { reason: "noted" })), []);
});

test("a looser budget needs a reason the base did not have", () => {
  for (const [base, head] of [
    [time(100), time(110)],
    [time(100), time(null)],
    [time(100), time(100, { tolerance: 0.5 })],
    [rate(100), rate(90)],
    [time(100), time(110, { reason: " " })],
    [time(100, { reason: "old" }), time(110, { reason: "old" })],
    [time(100, { reason: "old" }), time(110, { reason: "old " })],
  ]) {
    assert.deepEqual(errors(base, head), ["x has a looser budget without a new `reason` beside it"]);
  }
  assert.deepEqual(compare(time(100), time(110, { reason: "a bigger corpus" })), [
    { line: "x: 100 ms ±25% -> 110 ms ±25%, looser: a bigger corpus" },
  ]);
});

test("a budget added or removed is printed", () => {
  assert.deepEqual(compare({}, time(100)), [{ line: "x: new, 100 ms ±25%" }]);
  assert.deepEqual(compare(time(100), {}), [{ line: "x: removed" }]);
  assert.deepEqual(compare(time(100), time(100)), []);
});

test("the peak is read from what both tools print", () => {
  assert.equal(peakRss("x\nRSS, MiB: peak 89.4\ny"), 89.4);
  assert.equal(peakRss("RSS, MiB: before soak 26.6, peak 371.4, after 0 s idle 26.6"), 371.4);
});

test("the WASM compiler budget is ratcheted and cannot disappear", () => {
  const gate = (n) => `test $(wc -c < $wasm) -le ${n}`;
  assert.equal(wasmBudget(gate(4850000)), 4850000);
  assert.throws(() => wasmBudget(""), /missing compiler WASM budget/);
  const size = (n) => ({ "wasm.compiler.raw": { better: "lower", unit: "bytes", budget: wasmBudget(gate(n)), tolerance: 0 } });
  assert.deepEqual(errors(size(5303140), size(4850000)), []);
  assert.equal(errors(size(4850000), size(5303140)).length, 1);
});
