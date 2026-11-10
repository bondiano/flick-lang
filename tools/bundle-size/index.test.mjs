import assert from "node:assert/strict";
import { test } from "node:test";
import { attribute, costGroup } from "./index.mjs";

test("source attribution distinguishes adapter, kernel, Effect and rendering", () => {
  assert.equal(costGroup("/repo/runtime/src/rt/fiber.ts"), "runtime");
  assert.equal(costGroup("/repo/runtime/src/rt/ui/react.ts"), "ui");
  assert.equal(costGroup("/repo/packages/effect-ts/src/owner.ts"), "adapter");
  assert.equal(costGroup("/repo/node_modules/.pnpm/effect@4/node_modules/effect/dist/Effect.js"), "effect");
  assert.equal(costGroup("C:\\repo\\node_modules\\react-dom\\client.js"), "ui");
  assert.equal(costGroup("/repo/build/App/Main.js"), "other");
});

test("source attribution converts UTF-16 map columns to UTF-8 bytes", () => {
  const code = 'const text = "😀é";';
  const attributed = attribute(code, { sources: ["Main.js"], mappings: "AAAA" });
  assert.equal(attributed.get("Main.js"), Buffer.byteLength(code));
});
