// Run the language's independent assertions through the real JS component.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const compiler = resolve(process.argv[2] ?? join(root, "target/e2e/flick"));
const component = resolve(process.argv[3] ?? join(root, "target/backends/js.wasm"));
const temporary = join(root, "target/tmp");
mkdirSync(temporary, { recursive: true });
const project = mkdtempSync(join(temporary, "backend-js-conformance-"));
for (const dir of ["src", "test"]) {
  cpSync(join(root, "tests/language", dir), join(project, dir), { recursive: true });
}
const bytes = readFileSync(component);
writeFileSync(join(project, "backend.wasm"), bytes);
const digest = createHash("sha256").update(bytes).digest("hex");
const configuration = readFileSync(join(root, "tests/language/flick.toml"), "utf8");
writeFileSync(join(project, "flick.toml"), `${configuration}\n[build.backend]\nid = "flick/js"\nversion = "0.1.0"\nsha256 = "${digest}"\ncomponent = "backend.wasm"\n`);
let baseline;
for (const runtime of ["effect", "rt"]) {
  for (const profile of ["debug", "release"]) {
    const label = `${runtime}-${profile}`;
    const args = ["test", "--emit", "js", "--profile", profile, "--seed", "42", "--reporter", "json", "--offline"];
    if (runtime === "rt") args.push("--rt");
    const result = spawnSync(compiler, args, {
      cwd: project,
      env: { ...process.env, FLICK_STD_PATH: join(root, "packages/std") },
      encoding: "utf8", maxBuffer: 16 << 20,
    });
    writeFileSync(join(project, `${label}.json`), result.stdout ?? "");
    writeFileSync(join(project, `${label}.log`), result.stderr ?? "");
    assert.equal(result.error, undefined, `${label}: ${result.error}`);
    assert.equal(result.status, 0, `${label}: ${result.stderr}\nartifacts: ${project}`);
    const report = JSON.parse(result.stdout);
    assert.equal(report.seed, 42);
    assert.ok(report.tests.length > 0, "the full corpus must execute");
    assert.ok(report.tests.every(test => test.status === "passed"), `${label}: ${result.stdout}`);
    if (baseline) assert.deepEqual(report, baseline, label);
    else baseline = report;
    // The shared component driver publishes this manifest for its chosen launch.
    assert.ok(readFileSync(join(project, ".flick/test/.flick-backend.cbor")).length > 0);
    console.log(`${label}: ${report.tests.length} assertions passed through flick/js@0.1.0#${digest}`);
  }
}
console.log(`Artifacts: ${project}`);
