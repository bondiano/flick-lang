import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const fixtures = path.join(here, "fixtures");

test("effect-bump reports the change, writes the snapshot and moves the pins", () => {
  const root = mkdtempSync(path.join(tmpdir(), "effect-bump-test-"));
  cpSync(path.join(fixtures, "repo"), root, { recursive: true });
  const out = execFileSync(
    "node",
    [path.join(here, "../index.mjs"), "bump", "1.1.0", "--package-dir", path.join(fixtures, "effect-1.1.0"), "--root", root],
    { encoding: "utf8" },
  );
  assert.equal(
    out.split("\n\n")[0],
    [
      "effect 1.0.0 -> 1.1.0",
      "entrypoints added (1):",
      "  effect/extra",
      "exports added (1):",
      "  effect Sched.added",
      "exports removed (2):",
      "  effect Effect.sync",
      "  effect Sched.removed",
      "exports changed (1):",
      "  effect Sched.forever",
      "capability matrix missing (1):",
      "  Effect.sync",
    ].join("\n"),
  );
  const written = readFileSync(path.join(root, "stdlib/Effect/snapshot.json"), "utf8");
  // the coverage linter's fixture: `packages/effect-ts/tools/surface/tests/coverage.rs` lints it red
  assert.equal(written, readFileSync(path.join(fixtures, "effect-1.1.0.snapshot.json"), "utf8"));
  const pins = {
    "packages/effect-ts/package.json": '"effect": "1.1.0"',
    "packages/effect-ts/src/capabilities.ts": 'EFFECT_VERSION = "1.1.0"',
    "packages/effect-ts/test/effect-version.test.ts": 'PINNED_EFFECT = "1.1.0"',
    "flick/src/toolchain.rs": 'EFFECT: &str = "1.1.0"',
  };
  for (const [file, pin] of Object.entries(pins)) {
    assert.ok(readFileSync(path.join(root, file), "utf8").includes(pin), file);
  }
});
