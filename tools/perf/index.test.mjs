import assert from "node:assert/strict";
import { test } from "node:test";
import { METRICS, peakRss, report } from "./index.mjs";

const budgets = Object.fromEntries(Object.keys(METRICS).map((key) => [key, { budget: null, tolerance: 0.25 }]));

test("a stopped run retains measurements, budget failures and its cause", () => {
  const result = report({ "check.corpus": 26271 }, { ...budgets, "check.corpus": { budget: 3400, tolerance: 0.25 } }, ["SSR build failed"]);
  assert.deepEqual(result.rows, [{ metric: "check.corpus", value: 26271, unit: "ms" }]);
  assert.ok(result.errors.includes("SSR build failed"));
  assert.ok(result.errors.some((e) => e.includes("check.corpus is 26271")));
  assert.ok(result.errors.some((e) => e.includes("load.ssr.rps was not measured")));
  assert.match(result.table, /load.ssr.rps \| — .*not measured/);
});

test("every metric must be finite and have a budget entry", () => {
  const values = Object.fromEntries(Object.keys(METRICS).map((key) => [key, 1]));
  assert.deepEqual(report(values, budgets).errors, []);
  assert.ok(report({ ...values, "check.corpus": NaN }, budgets).errors.some((e) => e.includes("check.corpus was not measured")));
  assert.ok(report(values, {}).errors.some((e) => e.includes("has no entry")));
});

test("unavailable RSS stays missing without discarding the other measurements", () => {
  assert.ok(Number.isNaN(peakRss("hover, 50 samples p95 0.3 ms")));
});
