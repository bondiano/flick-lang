import assert from "node:assert/strict";
import test from "node:test";
import { validateReport } from "./report.mjs";

const complete = () => ({
  seed: 42, passed: 476, failed: 0,
  tests: Array.from({ length: 476 }, (_, i) => ({ name: `assertion-${i}`, status: "passed" })),
});

test("the complete independent corpus is accepted", () => {
  assert.doesNotThrow(() => validateReport(complete(), "effect-debug"));
});

test("a uniformly truncated corpus is refused", () => {
  const report = complete();
  report.tests = report.tests.slice(0, 1);
  report.passed = 1;
  assert.throws(() => validateReport(report, "effect-debug"));
});

test("the corpus without the four matcher cases is refused", () => {
  const report = complete();
  report.tests = report.tests.slice(0, 472);
  report.passed = 472;
  assert.throws(() => validateReport(report, "effect-debug"));
});

test("repeating one passing assertion cannot replace the corpus", () => {
  const report = complete();
  report.tests.fill(report.tests[0]);
  assert.throws(() => validateReport(report, "rt-release"));
});

test("a failed assertion is refused", () => {
  const report = complete();
  report.tests[0].status = "failed";
  report.passed = 475;
  report.failed = 1;
  assert.throws(() => validateReport(report, "rt-debug"));
});
