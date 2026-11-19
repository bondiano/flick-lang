// Rebuilds are compared byte-for-byte; updating a distribution pin is explicit.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const [mode, backend] = process.argv.slice(2);
assert.ok(["check", "update"].includes(mode) && /^[a-z]+$/.test(backend ?? ""), "usage: backend-package check|update <backend>");
const built = readFileSync(join(root, `target/backends/${backend}.wasm`));
assert.ok(built.subarray(0, 8).equals(Buffer.from([0, 97, 115, 109, 13, 0, 1, 0])), "a validated component is required");
const directory = join(root, `backends/${backend}/package`);
const version = /^version = "([^"]+)"/m.exec(readFileSync(join(root, `backends/${backend}/Cargo.toml`), "utf8"))?.[1];
assert.ok(version, `backends/${backend}/Cargo.toml must name a version`);
const id = `flick/${backend}`;
// what the descriptor accepts, from `[package.metadata.flick]` of the backend
const cargo = readFileSync(join(root, `backends/${backend}/Cargo.toml`), "utf8");
const range = (key) => {
  const found = new RegExp(`^${key} = \\{ minimum = "([^"]+)", maximum-exclusive = "([^"]+)" \\}`, "m").exec(cargo);
  assert.ok(found, `backends/${backend}/Cargo.toml must name ${key} in [package.metadata.flick]`);
  return { minimum: found[1], maximum_exclusive: found[2] };
};
const manifest = {
  identity: { id, version },
  sha256: createHash("sha256").update(built).digest("hex"),
  backend_api: range("backend-api"),
  ir: range("ir"),
};
if (mode === "update") {
  mkdirSync(directory, { recursive: true });
  writeFileSync(join(directory, "component.wasm"), built);
  writeFileSync(join(directory, "backend.json"), `${JSON.stringify(manifest, null, 2)}\n`);
} else {
  const distributed = readFileSync(join(directory, "component.wasm"));
  const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
  assert.ok(distributed.equals(built), `the distributed component must match the pinned Rust/WIT sources; run just update-component ${backend} (distributed ${distributed.length} bytes #${digest(distributed)}, rebuilt ${built.length} bytes #${manifest.sha256})`);
  assert.deepEqual(JSON.parse(readFileSync(join(directory, "backend.json"), "utf8")), manifest, "the distributed backend pin must match its exact bytes");
}
console.log(`${id}@${version}#${manifest.sha256}: ${mode}`);
