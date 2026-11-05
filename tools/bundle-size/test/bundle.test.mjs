import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";

import { build, measure } from "../index.mjs";

const repo = fileURLToPath(new URL("../../..", import.meta.url));
const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");

test("importing only Effect from the runtime keeps no Schema", async () => {
  const { modules } = await measure(fileURLToPath(new URL("effect-only.mjs", import.meta.url)));
  assert.ok(modules.some((id) => id.includes("effect/dist/Effect.js")));
  assert.deepEqual(modules.filter((id) => /Schema|runtime\/dist\/schema\//.test(id)), []);
});

test("an unused `deriving Show, Eq` adds no bytes to the bundle", async () => {
  const sizes = [];
  for (const name of ["plain", "derived"]) {
    const out = path.join(repo, "target/tmp/bundle-size", name);
    build(flick, `tools/bundle-size/test/unused-deriving/${name}.fl`, out, "release");
    sizes.push((await measure(path.join(out, "flick-main.js"))).min);
  }
  assert.equal(sizes[1], sizes[0]);
});

test("`deriving Default` through `Generic` is as small as the instance written by hand", async () => {
  const sizes = [];
  for (const name of ["hand", "derived"]) {
    const out = path.join(repo, "target/tmp/bundle-size/generic-default", name);
    build(flick, `tools/bundle-size/test/generic-default/${name}.fl`, out, "release");
    sizes.push((await measure(path.join(out, "flick-main.js"))).min);
  }
  assert.equal(sizes[1], sizes[0]);
});
