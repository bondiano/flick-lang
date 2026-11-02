import assert from "node:assert/strict";
import { test } from "node:test";

import effectPackage from "effect/package.json" with { type: "json" };
import runtimePackage from "../package.json" with { type: "json" };

// The adapter's contracts were verified against exactly this Effect release.
const PINNED_EFFECT = "4.1.2";

test("runtime declares Effect as an exact version", () => {
  assert.equal(runtimePackage.dependencies.effect, PINNED_EFFECT);
});

test("resolved Effect is the pinned version", () => {
  assert.equal(effectPackage.version, PINNED_EFFECT);
});
