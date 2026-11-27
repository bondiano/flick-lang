// Runs the kernel's conformance suite, the modules of tests/kernel that
// tests/kernel/lua.txt lists and those of tests/language that
// tests/language/lua.txt lists, with `flick test --target lua` on Lua 5.1.5
// and on LuaJIT 2.1 with the JIT on and off. Each JSON report must be the
// report of the same modules on JS, whose expectations the suites write
// from the contract, but for the effect tests JS runs: until the Lua
// runtime exists, the Lua runner reports each of them skipped, with the
// one reason `EFFECT`. Then `luac -l` lists the globals each kernel
// chunk reads: every one must be in the API snapshot of every profile.
//
// usage: node backends/lua/fixtures/kernel.mjs FLICK
// Hosts come from PATH or LUA51, LUAJIT and LUAC; a host of another version
// fails the run.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, cpSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { delimiter, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const fixtures = dirname(fileURLToPath(import.meta.url));
const lua = resolve(fixtures, "..");
const root = resolve(lua, "../..");
const out = join(root, "target/lua-kernel");
const flick = process.argv[2] ? resolve(process.argv[2]) : join(root, "target/e2e/flick");
const env = { ...process.env, FLICK_STD_PATH: process.env.FLICK_STD_PATH || join(root, "packages/std") };

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 600_000, ...options });
  if (result.error?.code === "ENOENT") throw new Error(`${command} is not on PATH: run in \`nix develop\` or set LUA51, LUAJIT and LUAC`);
  if (result.error) throw new Error(`${command}: ${result.error.message}`);
  return result;
}

/** The absolute path of `command`, as PATH or its variable names it. */
function located(variable, command) {
  const named = process.env[variable];
  if (named) return resolve(named);
  for (const dir of (process.env.PATH || "").split(delimiter)) {
    const path = join(dir, command);
    try {
      if (statSync(path).isFile()) return path;
    } catch {}
  }
  throw new Error(`${command} is not on PATH: run in \`nix develop\` or set ${variable}`);
}

function version(command, pattern) {
  const result = run(command, ["-v"]);
  const text = `${result.stdout}${result.stderr}`.trim().split("\n")[0];
  if (!pattern.test(text)) throw new Error(`${command} is \`${text}\`, not the pinned host ${pattern}`);
}

const lua51 = located("LUA51", "lua");
const luajit = located("LUAJIT", "luajit");
const luac = located("LUAC", "luac");
version(lua51, /^Lua 5\.1\.5 /);
version(luac, /^Lua 5\.1\.5 /);
version(luajit, /^LuaJIT 2\.1\./);

/** A suite's directory and the test modules of its `lua.txt`. */
const suites = [
  ["tests/kernel", "Kernel"],
  ["tests/language", "Language"],
].map(([dir, prefix]) => ({
  dir: join(root, dir),
  modules: readFileSync(join(root, dir, "lua.txt"), "utf8")
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"))
    .map((name) => `test/${prefix}/${name}.fl`),
}));

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

/** The JSON report of `modules` on `target` in `dir`. */
function report(dir, modules, target, path) {
  const args = ["test", "--target", target, "--seed", "1", "--reporter", "json", ...modules];
  const result = run(flick, args, { cwd: dir, env: { ...env, PATH: path } });
  if (!result.stdout.startsWith("{")) {
    throw new Error(`flick test --target ${target} wrote no report:\n${result.stderr}`);
  }
  return result.stdout;
}

// why the Lua runner skips an effect test JS runs (kernel/runner.lua)
const EFFECT = "an effect test needs the Lua runtime";

/**
 * How the Lua report `got` differs from the JS report `expected`: each test
 * by its name, and the order of those both run; and the effect tests Lua
 * skipped.
 */
function compare(expected, got) {
  const want = JSON.parse(expected);
  const have = JSON.parse(got);
  const differences = want.seed === have.seed ? [] : [`seed ${have.seed}, not ${want.seed}`];
  const lua = new Map(have.tests.map((t) => [t.name, t]));
  const effects = new Set();
  for (const test of want.tests) {
    const other = lua.get(test.name);
    lua.delete(test.name);
    if (other === undefined) differences.push(`${test.name}: not on Lua`);
    else if (
      other.reason === EFFECT &&
      other.status === "skipped" &&
      test.status !== "skipped" &&
      Object.keys(other).length === 3
    ) {
      effects.add(test.name);
    } else if (JSON.stringify(other) !== JSON.stringify(test)) {
      differences.push(`${test.name}: ${JSON.stringify(other)}, not ${JSON.stringify(test)}`);
    }
  }
  for (const name of lua.keys()) differences.push(`${name}: not on JS`);
  const order = (tests) => tests.filter((t) => !effects.has(t.name)).map((t) => t.name).join("\n");
  if (order(want.tests) !== order(have.tests)) differences.push("the tests run in another order");
  return { differences, effects: effects.size };
}

// the comparison itself: a named effect skip passes, nothing else does
{
  const js = (tests) => JSON.stringify({ seed: 1, passed: 2, failed: 0, tests });
  const a = { name: "M > a", status: "passed" };
  const e = { name: "M > e", status: "passed" };
  const skip = { name: "M > e", status: "skipped", reason: EFFECT };
  assert.deepEqual(compare(js([a, e]), js([a, skip])), { differences: [], effects: 1 });
  assert.ok(compare(js([a, e]), js([a, { ...skip, reason: "other" }])).differences.length > 0);
  assert.ok(compare(js([a, e]), js([a, { ...skip, status: "failed" }])).differences.length > 0);
  assert.ok(compare(js([a, { ...e, status: "skipped" }]), js([a, skip])).differences.length > 0);
  assert.ok(compare(js([a, e]), js([a])).differences.length > 0);
  assert.ok(compare(js([a, e]), js([e, a])).differences.length > 0);
}

// each host alone in `[tools]`, first on PATH under the name flick looks for
const hosts = [
  { name: "Lua 5.1.5", tool: "lua", run: lua51 },
  { name: "LuaJIT 2.1", tool: "luajit", run: luajit },
  { name: "LuaJIT 2.1 -joff", tool: "luajit", run: luajit, flags: "-joff" },
];
let failed = 0;
for (const suite of suites) {
  const expected = report(suite.dir, suite.modules, "js", process.env.PATH);
  for (const host of hosts) {
    const dir = join(out, `${suite.dir.split("/").pop()}-${host.name.replace(/[^A-Za-z0-9]+/g, "-")}`);
    cpSync(suite.dir, dir, {
      recursive: true,
      filter: (path) => !path.includes(`${suite.dir}/.flick`) && !path.includes(`${suite.dir}/dist`),
    });
    const tool = `"flick/lua" = ["${host.tool}"]`;
    const original = readFileSync(join(dir, "flick.toml"), "utf8");
    const config = /"flick\/lua" = \[[^\]]*\]/.test(original)
      ? original.replace(/"flick\/lua" = \[[^\]]*\]/, tool)
      : original.replace("[tools]", `[tools]\n${tool}`);
    writeFileSync(join(dir, "flick.toml"), config);
    const bin = join(dir, ".bin");
    mkdirSync(bin);
    writeFileSync(join(bin, host.tool), `#!/bin/sh\nexec "${host.run}"${host.flags ? ` ${host.flags}` : ""} "$@"\n`);
    chmodSync(join(bin, host.tool), 0o755);
    const got = report(dir, suite.modules, "lua", `${bin}${delimiter}${process.env.PATH}`);
    const { differences, effects } = compare(expected, got);
    failed += differences.length > 0 ? 1 : 0;
    const skipped = effects > 0 ? ` (${effects} effect test${effects === 1 ? "" : "s"} skipped)` : "";
    console.log(`${differences.length > 0 ? "FAIL" : "pass"}  ${suite.modules.join(", ")} on ${host.name}${skipped}`);
    for (const difference of differences) console.log(`  ${difference}`);
  }
}

// the globals each kernel chunk reads, but the runner's, which reads the
// interpreter's `io` and `os` on a test host alone
const snapshots = ["lua-5.1", "luajit-2.1", "love-11.5", "wow-retail", "wow-forever"].map((name) => ({
  name,
  defined: new Set(readFileSync(join(lua, "profiles/api", `${name}.txt`), "utf8").split("\n")),
}));
for (const file of readdirSync(join(lua, "kernel")).filter((f) => f.endsWith(".lua") && f !== "runner.lua")) {
  const listing = run(luac, ["-p", "-l", join(lua, "kernel", file)]);
  if (listing.status !== 0) throw new Error(`luac rejects ${file}: ${listing.stderr}`);
  const globals = new Set([...listing.stdout.matchAll(/[GS]ETGLOBAL\s.*;\s*(\S+)/g)].map((m) => m[1]));
  const missing = [...globals].flatMap((g) => snapshots.filter((s) => !s.defined.has(g)).map((s) => `${g} (${s.name})`));
  failed += missing.length > 0 ? 1 : 0;
  console.log(`${missing.length > 0 ? "FAIL" : "pass"}  kernel/${file} reads ${[...globals].sort().join(", ")}`);
  for (const m of missing) console.log(`  not in a snapshot: ${m}`);
}
if (failed > 0) {
  console.log(`${failed} failed`);
  process.exit(1);
}
