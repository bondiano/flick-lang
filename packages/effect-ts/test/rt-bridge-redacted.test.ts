import assert from "node:assert/strict";
import { test } from "node:test";

import { redactedMake as nativeMake, redactedReveal as nativeReveal } from "@flick-lang/runtime/rt/redacted";
import { codecRedactedReveal } from "@flick-lang/runtime/rt/codec";
import { schemaString } from "@flick-lang/runtime/rt/schema";

import { redactedMake, redactedReveal } from "../src/stdlib/misc.ts";
import { flickEquivalent } from "../src/effect/Redacted.ts";

test("redacted values from either runtime retain their codec and explicit reveal policy", () => {
  const native = nativeMake("secret");
  const host = redactedMake("secret");
  assert.equal(nativeReveal(native), "secret");
  assert.equal(redactedReveal(native), "secret");
  assert.equal(flickEquivalent((a, b) => a === b, native, host), true);
  const codec = codecRedactedReveal(schemaString);
  assert.deepEqual(codec.encode(host), { _tag: "Success", success: "secret" });
  const decoded = codec.decode("secret");
  assert.equal(decoded._tag, "Success");
  if (decoded._tag === "Success") {
    assert.ok(typeof decoded.success === "object" && decoded.success !== null);
    assert.equal(JSON.stringify(decoded.success).includes("secret"), false);
  }
  assert.equal(JSON.stringify([host, native]).includes("secret"), false);
});
