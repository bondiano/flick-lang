// Runs the kernel's conformance suite, the modules of tests/kernel that
// tests/kernel/lua.txt lists, with `flick test --target lua` on Lua 5.1.5
// and on LuaJIT 2.1 with the JIT on and off. Each JSON report must be the
// report of the same modules on JS, whose expectations the suite writes
// from the kernel's contract. Then `luac -l` lists the globals each kernel
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
const suite = join(root, "tests/kernel");
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

const modules = readFileSync(join(suite, "lua.txt"), "utf8")
  .split("\n")
  .map((line) => line.trim())
  .filter((line) => line && !line.startsWith("#"))
  .map((name) => `test/Kernel/${name}.fl`);

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });

/** The JSON report of the suite's `modules` on `target` in `dir`. */
function report(dir, target, path) {
  const args = ["test", "--target", target, "--seed", "1", "--reporter", "json", ...modules];
  const result = run(flick, args, { cwd: dir, env: { ...env, PATH: path } });
  if (!result.stdout.startsWith("{")) {
    throw new Error(`flick test --target ${target} wrote no report:\n${result.stderr}`);
  }
  return result.stdout;
}

const expected = report(suite, "js", process.env.PATH);

// each host alone in `[tools]`, first on PATH under the name flick looks for
const hosts = [
  { name: "Lua 5.1.5", tool: "lua", run: lua51 },
  { name: "LuaJIT 2.1", tool: "luajit", run: luajit },
  { name: "LuaJIT 2.1 -joff", tool: "luajit", run: luajit, flags: "-joff" },
];
let failed = 0;
for (const host of hosts) {
  const dir = join(out, host.name.replace(/[^A-Za-z0-9]+/g, "-"));
  cpSync(suite, dir, { recursive: true, filter: (path) => !path.includes(`${suite}/.flick`) && !path.includes(`${suite}/dist`) });
  const config = readFileSync(join(dir, "flick.toml"), "utf8").replace(/"flick\/lua" = \[[^\]]*\]/, `"flick/lua" = ["${host.tool}"]`);
  writeFileSync(join(dir, "flick.toml"), config);
  const bin = join(dir, ".bin");
  mkdirSync(bin);
  writeFileSync(join(bin, host.tool), `#!/bin/sh\nexec "${host.run}"${host.flags ? ` ${host.flags}` : ""} "$@"\n`);
  chmodSync(join(bin, host.tool), 0o755);
  const got = report(dir, "lua", `${bin}${delimiter}${process.env.PATH}`);
  const passed = got === expected;
  failed += passed ? 0 : 1;
  console.log(`${passed ? "pass" : "FAIL"}  ${host.name}`);
  if (!passed) console.log(`  expected ${expected}  got      ${got}`);
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
