import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./index.mjs", import.meta.url));
const metadataName = "__wasm_bindgen_unstable";
const leb = (value) => {
  const bytes = [];
  do {
    const next = value & 127;
    value >>>= 7;
    bytes.push(next | (value ? 128 : 0));
  } while (value);
  return bytes;
};
const text = (value) => [...leb(Buffer.byteLength(value)), ...Buffer.from(value)];
const section = (id, bytes) => [id, ...leb(bytes.length), ...bytes];
const fixture = (metadata = true) => {
  // Both exports call an observable import, then compute 6 * 7. Deduplication
  // merges their identical bodies; size optimization also folds the product.
  const body = [0, 0x10, 0, 0x1a, 0x41, 6, 0x41, 7, 0x6c, 0x0b];
  return Buffer.from([
    0, 97, 115, 109, 1, 0, 0, 0,
    ...section(1, [1, 0x60, 0, 1, 0x7f]),
    ...section(2, [1, ...text("env"), ...text("touch"), 0, 0]),
    ...section(3, [2, 0, 0]),
    ...section(7, [2, ...text("left"), 0, 1, ...text("right"), 0, 2]),
    ...section(10, [2, ...leb(body.length), ...body, ...leb(body.length), ...body]),
    ...(metadata ? section(0, [...text(metadataName), 1, 2, 3, 4]) : []),
  ]);
};
const temporary = (t) => {
  const directory = mkdtempSync(join(tmpdir(), "flick-wasm-opt-"));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  return directory;
};

test("size optimization preserves exports, import effects and bindgen metadata", (t) => {
  const directory = temporary(t);
  const file = join(directory, "compiler.wasm");
  const deduplicated = join(directory, "deduplicated.wasm");
  const before = fixture();
  writeFileSync(file, before);
  execFileSync("wasm-opt", [file, "--duplicate-function-elimination", "--converge",
    "--strip-debug", "--strip-producers", "-o", deduplicated]);
  execFileSync(process.execPath, [script, file]);
  const after = readFileSync(file);
  assert.ok(after.length < readFileSync(deduplicated).length,
    "optimize instructions beyond duplicate elimination");
  const module = new WebAssembly.Module(after);
  const original = new WebAssembly.Module(before);
  assert.deepEqual(WebAssembly.Module.imports(module), WebAssembly.Module.imports(original));
  assert.deepEqual(WebAssembly.Module.exports(module), WebAssembly.Module.exports(original));
  assert.deepEqual(WebAssembly.Module.customSections(module, metadataName),
    WebAssembly.Module.customSections(original, metadataName));
  let calls = 0;
  const { exports } = new WebAssembly.Instance(module, { env: { touch: () => ++calls } });
  assert.equal(exports.left(), 42);
  assert.equal(exports.right(), 42);
  assert.equal(calls, 2);
});

test("lost bindgen metadata rejects the output without replacing the input", (t) => {
  const directory = temporary(t);
  const file = join(directory, "compiler.wasm");
  const output = join(directory, "without-metadata.wasm");
  const before = fixture();
  writeFileSync(file, before);
  writeFileSync(output, fixture(false));
  const optimizer = join(directory, "wasm-opt");
  writeFileSync(optimizer, `#!${process.execPath}\nimport { copyFileSync } from "node:fs";
if (process.argv.includes("--version")) console.log("wasm-opt version 133");
else copyFileSync(process.env.OPTIMIZER_OUTPUT, process.argv[process.argv.indexOf("-o") + 1]);
`, { mode: 0o755 });
  const result = spawnSync(process.execPath, [script, file], {
    encoding: "utf8",
    env: { ...process.env, PATH: `${directory}${delimiter}${process.env.PATH}`,
      OPTIMIZER_OUTPUT: output },
  });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /metadata/);
  assert.deepEqual(readFileSync(file), before);
  assert.ok(!readdirSync(directory).some((name) => name.endsWith(".dedup")));
});
