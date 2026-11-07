import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");
const flick = process.env.FLICK_BIN ?? path.join(process.env.CARGO_TARGET_DIR ?? path.join(repo, "target"), "debug/flick");

const check = (file: string) => spawnSync(flick, ["check", file, "--color", "never"], { encoding: "utf8" });

test("the signature on the landing page holds, and taking Log out of it is FLICK-E002", async () => {
  const out = path.join(repo, "target/tmp/hero");
  rmSync(out, { recursive: true, force: true });
  execFileSync(flick, ["build", path.join(here, "../src/widgets/Hero.fl"), "--out-dir", out, "--color", "never"]);
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the compiled module exports what `Hero.fl` exposes
  const hero = (await import(pathToFileURL(path.join(out, "Hero.js")).href)) as { honest_source: string; lying_source: string };

  const programs = path.join(out, "programs");
  mkdirSync(programs);
  const honest = path.join(programs, "honest.fl");
  const lying = path.join(programs, "lying.fl");
  writeFileSync(honest, hero.honest_source);
  writeFileSync(lying, hero.lying_source);

  const truth = check(honest);
  assert.equal(truth.status, 0, truth.stderr);
  const lie = check(lying);
  assert.equal(lie.status, 1);
  assert.match(lie.stderr, /FLICK-E002/);
  assert.match(lie.stderr, /service `Log`/);
});

test("the code samples on the landing page compile", () => {
  const samples = path.join(here, "../src/widgets/samples");
  for (const name of readdirSync(samples).filter((file) => file.endsWith(".fl"))) {
    const result = check(path.join(samples, name));
    assert.equal(result.status, 0, `${name}: ${result.stderr}`);
  }
});
