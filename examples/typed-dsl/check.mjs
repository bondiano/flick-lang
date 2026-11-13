// Run from anywhere after building the component (see README).
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
const root = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(root, "../..");
const bin = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");
copyFileSync(path.join(repo, "target/wasm32-wasip2/release/flick_example_typed_dsl.wasm"), path.join(root, "library/validation.wasm"));
const source = path.join(root, "app/src/Main.fl");
const original = readFileSync(source, "utf8");
const check = () => spawnSync(bin, ["check", "--diagnostics", "json"], { cwd: path.join(root, "app"), encoding: "utf8" });
try {
  const accepted = check();
  assert.equal(accepted.status, 0, accepted.stderr);
  for (const [body, code] of [["negative 42", "validation/V001"], ['positive "42"', "FLICK-T003"]]) {
    writeFileSync(source, original.replace("positive 42", body));
    const result = check();
    assert.notEqual(result.status, 0);
    assert.ok(result.stderr.includes(code), result.stderr);
  }
  for (const value of ["42", "0"]) {
    writeFileSync(source, original.replace("positive 42", `positive ${value}`));
    assert.equal(check().status, 0);
    const result = execFileSync(bin, ["run"], { cwd: path.join(root, "app"), encoding: "utf8" });
    assert.equal(result.trim(), value === "42" ? "Ok 42" : "Err NotPositive");
  }
} finally { writeFileSync(source, original); }
