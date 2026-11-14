// Rebuilds are compared byte-for-byte; updating a distribution pin is explicit.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const mode = process.argv[2];
assert.ok(["check", "update"].includes(mode), "usage: backend-package check|update");
const built = readFileSync(join(root, "target/backends/js.wasm"));
assert.ok(built.subarray(0, 8).equals(Buffer.from([0, 97, 115, 109, 13, 0, 1, 0])), "a validated component is required");
const directory = join(root, "backends/js/package");
const manifest = {
  identity: { id: "flick/js", version: "0.1.0" },
  sha256: createHash("sha256").update(built).digest("hex"),
};
if (mode === "update") {
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, "component.wasm"), built);
  writeFileSync(join(directory, "backend.json"), `${JSON.stringify(manifest, null, 2)}\n`);
} else {
  const distributed = readFileSync(join(directory, "component.wasm"));
  const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
  assert.ok(distributed.equals(built), `the distributed component must match the pinned Rust/WIT sources; run just update-js-component (distributed ${distributed.length} bytes #${digest(distributed)}, rebuilt ${built.length} bytes #${manifest.sha256})`);
  assert.deepEqual(JSON.parse(readFileSync(join(directory, "backend.json"), "utf8")), manifest, "the distributed backend pin must match its exact bytes");
}
console.log(`flick/js@0.1.0#${manifest.sha256}: ${mode}`);
