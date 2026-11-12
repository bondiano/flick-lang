import assert from "node:assert/strict";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { execFileSync } from "node:child_process";

import { build, measure } from "../index.mjs";

const repo = fileURLToPath(new URL("../../..", import.meta.url));
const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");

test("runtime size comparisons execute the same I/O program on both backends", () => {
  for (const rt of [false, true]) {
    const out = path.join(repo, "target/tmp/bundle-size", rt ? "io-rt" : "io-effect");
    build(flick, "tools/bundle-size/pure.fl", out, "release", undefined, rt);
    assert.equal(execFileSync(process.execPath, [path.join(out, "flick-main.js")], { encoding: "utf8" }), "24.560000000000002\n");
  }
});

test("action span construction is retained only by roots that opt in", async () => {
  const off = await measure(fileURLToPath(new URL("untraced-ui.mjs", import.meta.url)));
  const on = await measure(fileURLToPath(new URL("traced-ui.mjs", import.meta.url)));
  const tracing = (id) => /\/effect-ts\/dist\/ui\/tracing\.js$/.test(id.replaceAll("\\", "/"));
  assert.equal(off.modules.some(tracing), false);
  assert.equal(on.modules.some(tracing), true);
  assert.ok(on.min > off.min);
});

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
