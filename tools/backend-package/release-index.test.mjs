import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";

/** A checkout holding the script and a pin of `flick/js`, the index in it. */
const checkout = (t) => {
  const root = mkdtempSync(join(tmpdir(), "backends-index-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, "tools/backend-package"), { recursive: true });
  mkdirSync(join(root, "backends/js/package"), { recursive: true });
  copyFileSync(new URL("./release-index.mjs", import.meta.url), join(root, "tools/backend-package/release-index.mjs"));
  const pin = {
    identity: { id: "flick/js", version: "0.1.0" },
    sha256: "c".repeat(64),
    backend_api: { minimum: "0.1.0", maximum_exclusive: "0.2.0" },
    ir: { minimum: "0.13.0", maximum_exclusive: "0.14.0" },
  };
  writeFileSync(join(root, "backends/js/package/backend.json"), JSON.stringify(pin));
  const script = join(root, "tools/backend-package/release-index.mjs");
  const file = join(root, "backends.json");
  return { file, run: (url) => spawnSync(process.execPath, [script, "js", url, file], { encoding: "utf8" }) };
};

test("the index gains the published component and keeps the others", (t) => {
  const { file, run } = checkout(t);
  const other = { id: "flick/go", version: "0.1.0", sha256: "ab", url: "u", backend_api: {}, ir: {} };
  const stale = { id: "flick/js", version: "0.1.0", sha256: "old", url: "old", backend_api: {}, ir: {} };
  writeFileSync(file, JSON.stringify({ backends: [stale, other] }));
  const result = run("https://example/component.wasm");
  assert.equal(result.status, 0, result.stderr);
  const index = JSON.parse(readFileSync(file, "utf8"));
  assert.deepEqual(index.backends.map((e) => e.id), ["flick/go", "flick/js"]);
  const js = index.backends[1];
  assert.equal(js.url, "https://example/component.wasm");
  assert.equal(js.sha256, "c".repeat(64));
  assert.equal(js.backend_api.minimum, "0.1.0");
});

test("a missing index starts empty", (t) => {
  const { file, run } = checkout(t);
  assert.equal(run("u").status, 0);
  assert.equal(JSON.parse(readFileSync(file, "utf8")).backends.length, 1);
});
