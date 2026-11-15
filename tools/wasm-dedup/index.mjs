// Optimize the compiler after linking several Rust crates. Keep every
// import/export and wasm-bindgen's metadata while minimizing instruction size.
import { execFileSync } from "node:child_process";
import { readFileSync, renameSync, rmSync } from "node:fs";

const file = process.argv[2];
if (!file) throw new Error("usage: node tools/wasm-dedup/index.mjs <compiler.wasm>");

const version = execFileSync("wasm-opt", ["--version"], { encoding: "utf8" }).trim();
if (!/^wasm-opt version 133(?:\s|$)/.test(version)) {
  throw new Error(`expected Binaryen 133, got ${version}; nix develop provides the pinned tool`);
}

const shape = (bytes) => {
  const module = new WebAssembly.Module(bytes);
  const ordered = (entries) => entries.map((entry) => JSON.stringify(entry)).sort();
  return JSON.stringify({
    imports: ordered(WebAssembly.Module.imports(module)),
    exports: ordered(WebAssembly.Module.exports(module)),
  });
};
const metadata = (bytes) => JSON.stringify(WebAssembly.Module.customSections(
  new WebAssembly.Module(bytes), "__wasm_bindgen_unstable",
).map((section) => Buffer.from(section).toString("hex")));
const before = readFileSync(file);
const temporary = `${file}.${process.pid}.dedup`;
try {
  execFileSync("wasm-opt", [
    file,
    "--duplicate-function-elimination", "--converge", "-Oz",
    "--strip-debug", "--strip-producers",
    "-o", temporary,
  ], { stdio: "inherit" });
  const after = readFileSync(temporary);
  if (shape(after) !== shape(before)) throw new Error("WASM optimization changed imports or exports");
  if (metadata(after) !== metadata(before)) throw new Error("WASM optimization changed wasm-bindgen metadata");
  if (after.length > before.length) throw new Error("WASM optimization increased the artifact size");
  renameSync(temporary, file);
  console.log(`WASM size optimization: ${before.length} → ${after.length} bytes (${version})`);
} finally {
  rmSync(temporary, { force: true });
}
