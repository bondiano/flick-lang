import assert from "node:assert/strict";

export function validateReport(report, label) {
  assert.equal(report.seed, 42);
  // Update this deliberately when adding or removing language assertions.
  const expected = 510;
  assert.equal(report.tests.length, expected, "the full corpus must execute");
  assert.equal(new Set(report.tests.map(test => test.name)).size, expected, "assertions must be distinct");
  assert.equal(report.passed, expected);
  assert.equal(report.failed, 0);
  assert.ok(report.tests.every(test => test.status === "passed"), `${label}: ${JSON.stringify(report)}`);
}
