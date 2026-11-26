// Runs the kernel's conformance suite, the modules of tests/kernel that
// tests/kernel/lua.txt lists and those of tests/language that
// tests/language/lua.txt lists, with `flick test --target lua` on Lua 5.1.5
// and on LuaJIT 2.1 with the JIT on and off. Each JSON report must be the
// report of the same modules on JS, whose expectations the suites write
// from the contract. Then `luac -l` lists the globals each kernel
// chunk reads: every one must be in the API snapshot of every profile.
//
// usage: node backends/lua/fixtures/kernel.mjs FLICK
// Hosts come from PATH or LUA51, LUAJIT and LUAC; a host of another version
// fails the run.
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
    const passed = got === expected;
    failed += passed ? 0 : 1;
    console.log(`${passed ? "pass" : "FAIL"}  ${suite.modules.join(", ")} on ${host.name}`);
    if (!passed) console.log(`  expected ${expected}  got      ${got}`);
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
