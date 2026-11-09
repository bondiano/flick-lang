import assert from "node:assert/strict";
import { test } from "node:test";

import { flickEquivalent } from "../src/rt/effect/Redacted.ts";
import { redactedMake, redactedReveal } from "../src/rt/redacted.ts";

test("redacted contents need an explicit reveal", () => {
  const secret = redactedMake("private-value");
  assert.equal(redactedReveal(secret), "private-value");
  assert.equal(String(secret), "<redacted>");
  assert.equal(JSON.stringify(secret), '"<redacted>"');
  assert.ok(flickEquivalent((a: string, b: string) => a === b, secret, redactedMake("private-value")));
  assert.ok(!flickEquivalent((a: string, b: string) => a === b, secret, redactedMake("other")));
});
