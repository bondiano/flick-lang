import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

// An empty, valid WebAssembly component; the changed one adds a custom
// section without changing its ABI. Even that change must fail the pin.
const component = Buffer.from([0, 97, 115, 109, 13, 0, 1, 0]);

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), "flick-component-pin-"));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const script = join(root, "tools/backend-package/index.mjs");
  mkdirSync(join(root, "tools/backend-package"), { recursive: true });
  mkdirSync(join(root, "target/backends"), { recursive: true });
  mkdirSync(join(root, "backends/js/package"), { recursive: true });
  copyFileSync(new URL("./index.mjs", import.meta.url), script);
  writeFileSync(
    join(root, "backends/js/Cargo.toml"),
    'version = "0.1.0"\n\n[package.metadata.flick]\nbackend-api = { minimum = "0.1.0", maximum-exclusive = "0.2.0" }\nir = { minimum = "0.12.0", maximum-exclusive = "0.13.0" }\n',
  );
  const built = join(root, "target/backends/js.wasm");
  const distributed = join(root, "backends/js/package/component.wasm");
  const manifest = join(root, "backends/js/package/backend.json");
  const metadata = {
    identity: { id: "flick/js", version: "0.1.0" },
    sha256: createHash("sha256").update(component).digest("hex"),
    backend_api: { minimum: "0.1.0", maximum_exclusive: "0.2.0" },
    ir: { minimum: "0.12.0", maximum_exclusive: "0.13.0" },
  };
  writeFileSync(built, component);
  writeFileSync(distributed, component);
  writeFileSync(manifest, JSON.stringify(metadata));
  return {
    built, distributed, manifest, metadata,
    check: () => spawnSync(process.execPath, [script, "check", "js"], { encoding: "utf8" }),
  };
}

test("pin accepts exact component bytes and identity", (t) => {
  const f = fixture(t);
  const result = f.check();
  assert.equal(result.status, 0, result.stderr);
});

test("pin rejects changed bytes even when a custom section preserves the ABI", (t) => {
  const f = fixture(t);
  writeFileSync(f.built, Buffer.concat([component, Buffer.from([0, 2, 1, 120])]));
  const result = f.check();
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /distributed component must match/);
  assert.deepEqual(readFileSync(f.distributed), component);
  assert.deepEqual(JSON.parse(readFileSync(f.manifest, "utf8")), f.metadata);
});

test("pin rejects an incorrect digest without updating it", (t) => {
  const f = fixture(t);
  f.metadata.sha256 = "0".repeat(64);
  writeFileSync(f.manifest, JSON.stringify(f.metadata));
  assert.notEqual(f.check().status, 0);
  assert.deepEqual(JSON.parse(readFileSync(f.manifest, "utf8")), f.metadata);
});

test("pin rejects a different backend identity", (t) => {
  const f = fixture(t);
  f.metadata.identity.version = "0.2.0";
  writeFileSync(f.manifest, JSON.stringify(f.metadata));
  assert.notEqual(f.check().status, 0);
});
