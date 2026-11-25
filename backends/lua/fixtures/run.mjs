// Builds each fixture of this directory through the committed Lua component
// and runs it on the pinned hosts: Lua 5.1.5, LuaJIT 2.1 with the JIT on and
// off, and LÖVE 11.5 headless for `lua/love`; for the WoW profiles, which no
// host here runs, `luac -p` of every chunk and Lua 5.1 loading the chunks in
// the order a `.toc` lists them. Each output must equal the fixture's
// independently written `expected`; an error must be the fixture's `error`,
// its location mapped back to Flick through the chunk's source map.
//
// usage: node backends/lua/fixtures/run.mjs FLICK
// Hosts come from PATH or LUA51, LUAJIT, LUAC and LOVE; a host of another
// version fails the run.
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const fixtures = dirname(fileURLToPath(import.meta.url));
const lua = resolve(fixtures, "..");
const root = resolve(lua, "../..");
const out = join(root, "target/lua-run");
const flick = process.argv[2] ? resolve(process.argv[2]) : join(root, "target/e2e/flick");
const host = (name, fallback) => process.env[name] || fallback;
const [lua51, luajit, luac, love] = [host("LUA51", "lua"), host("LUAJIT", "luajit"), host("LUAC", "luac"), host("LOVE", "love")];
const headless = { ...process.env, SDL_VIDEODRIVER: "dummy", SDL_AUDIODRIVER: "dummy", ALSOFT_DRIVERS: "null" };
const env = { ...process.env, FLICK_STD_PATH: process.env.FLICK_STD_PATH || join(root, "packages/std") };

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { encoding: "utf8", timeout: 120_000, ...options });
  if (result.error?.code === "ENOENT") throw new Error(`${command} is not on PATH: run in \`nix develop\` or set LUA51, LUAC, LUAJIT and LOVE`);
  if (result.error) throw new Error(`${command}: ${result.error.message}`);
  return result;
}

function version(command, args, pattern) {
  const result = run(command, args);
  const text = `${result.stdout}${result.stderr}`.trim().split("\n")[0];
  if (!pattern.test(text)) throw new Error(`${command} is \`${text}\`, not the pinned host ${pattern}`);
  return text;
}

const versions = {
  lua51: version(lua51, ["-v"], /^Lua 5\.1\.5 /),
  luac: version(luac, ["-v"], /^Lua 5\.1\.5 /),
  luajit: version(luajit, ["-v"], /^LuaJIT 2\.1\./),
  love: version(love, ["--version"], /^LOVE 11\.5 /),
};

rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
const component = readFileSync(join(lua, "package/component.wasm"));
const sha = createHash("sha256").update(component).digest("hex");

// a host loading the chunks in the order of `chunks.txt`, each given the
// program's namespace second, as WoW gives an addon's table to each file
const shim = join(out, "shim.lua");
writeFileSync(shim, `local ns = {}
local ok, err = pcall(function()
  for path in io.lines("chunks.txt") do
    assert(loadfile(path))(nil, ns)
  end
end)
if not ok then
  io.stderr:write(tostring(err), "\\n")
  os.exit(1)
end
`);

// a game around the generated main.lua: love.load runs it, the first
// love.update quits with whether it raised
const conf = "function love.conf(t)\n  t.window = false\nend\n";
const game = `local failed = false
function love.load()
  local ok, err = pcall(function()
    assert(love.filesystem.load("flick-main.lua"))()
  end)
  if not ok then
    io.stderr:write(tostring(err), "\\n")
    failed = true
  end
end
function love.update()
  love.event.quit(failed and 1 or 0)
end
`;

const profiles = [
  { name: "love", environment: "lua/love", dialect: "lua51", hosts: ["lua51", "luajit", "luajit-joff", "love"] },
  { name: "love-luajit", environment: "lua/love", dialect: "luajit", hosts: ["luajit", "luajit-joff", "love"] },
  { name: "wow-retail", environment: "lua/wow-retail", dialect: "lua51", hosts: ["luac", "lua51"] },
  { name: "wow-forever", environment: "lua/wow-forever", dialect: "lua51", hosts: ["luac", "lua51"] },
];

function config(profile) {
  const dialect = profile.dialect === "luajit" ? '[target.lua]\ndialect = "luajit"\n\n' : "";
  return `schema-version = 1\n[project]\nkind = "app"\nentry = "App"\n\n${dialect}[build]\nenvironment = "${profile.environment}"\n\n[build.backend]\nid = "flick/lua"\nversion = "0.1.0"\nsha256 = "${sha}"\ncomponent = "backend.wasm"\n`;
}

/** The base64 VLQ values of one segment of a source map. */
function vlq(segment) {
  const digits = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  const values = [];
  let value = 0;
  let shift = 0;
  for (const c of segment) {
    const digit = digits.indexOf(c);
    value += (digit & 31) << shift;
    if (digit & 32) {
      shift += 5;
      continue;
    }
    values.push(value & 1 ? -(value >> 1) : value >> 1);
    value = 0;
    shift = 0;
  }
  return values;
}

/** The Flick source and 1-based line generated line `line` of `map` maps to. */
function source(map, line) {
  let [src, srcLine] = [0, 0];
  const lines = map.mappings.split(";");
  for (let i = 0; i < lines.length; i++) {
    for (const segment of lines[i].split(",").filter(Boolean)) {
      const [, ds, dl] = vlq(segment);
      src += ds;
      srcLine += dl;
      if (i === line - 1) return `${map.sources[src]}:${srcLine + 1}`;
    }
  }
  return undefined;
}

/** The error `stderr` reports, at its Flick source: `src/App.fl:7: message`;
 * a chunk is named by its path in `dist`, where the host loads it. */
function located(stderr, dist) {
  const lines = stderr.split("\n");
  const at = lines.map((l) => /^(.+?\.lua):(\d+): (.*)$/.exec(l)).find(Boolean);
  if (!at) return `unlocated: ${lines[0]}`;
  const chunk = at[1];
  const file = join(dist, `${chunk}.map`);
  if (!existsSync(file)) return `no map for ${chunk}: ${at[0]}`;
  const flickSource = source(JSON.parse(readFileSync(file, "utf8")), Number(at[2]));
  return `${flickSource ?? `${chunk}:${at[2]} unmapped`}: ${at[3]}`;
}

function runOn(name, dist, chunks) {
  switch (name) {
    case "lua51":
      return run(lua51, [shim], { cwd: dist });
    case "luajit":
      return run(luajit, [shim], { cwd: dist });
    case "luajit-joff":
      return run(luajit, ["-joff", shim], { cwd: dist });
    case "luac":
      return run(luac, ["-p", ...chunks.map((c) => join(dist, c))]);
    case "love": {
      const dir = `${dist}-game`;
      rmSync(dir, { recursive: true, force: true });
      cpSync(dist, dir, { recursive: true });
      renameSync(join(dir, "main.lua"), join(dir, "flick-main.lua"));
      writeFileSync(join(dir, "conf.lua"), conf);
      writeFileSync(join(dir, "main.lua"), game);
      return run(love, [dir], { env: headless });
    }
    default:
      throw new Error(`unknown host ${name}`);
  }
}

const names = readdirSync(fixtures, { withFileTypes: true }).filter((e) => e.isDirectory()).map((e) => e.name).sort();
const results = [];
let failed = 0;
for (const fixture of names) {
  const dir = join(fixtures, fixture);
  const expected = readFileSync(join(dir, "expected"), "utf8");
  const error = existsSync(join(dir, "error")) ? readFileSync(join(dir, "error"), "utf8").trim() : undefined;
  for (const profile of profiles) {
    const project = join(out, fixture, profile.name);
    mkdirSync(project, { recursive: true });
    cpSync(join(dir, "src"), join(project, "src"), { recursive: true });
    writeFileSync(join(project, "flick.toml"), config(profile));
    writeFileSync(join(project, "backend.wasm"), component);
    const built = run(flick, ["build", "--offline"], { cwd: project, env, timeout: 600_000 });
    if (built.status !== 0) {
      failed++;
      console.error(`FAIL ${fixture} on ${profile.name}: the build failed\n${built.stdout}${built.stderr}`);
      results.push({ fixture, profile: profile.name, host: "build", pass: false });
      continue;
    }
    const dist = join(project, "dist");
    const chunks = readFileSync(join(dist, "chunks.txt"), "utf8").split("\n").filter(Boolean);
    for (const h of profile.hosts) {
      const got = runOn(h, dist, chunks);
      let pass;
      let detail = "";
      if (h === "luac") {
        pass = got.status === 0;
        detail = got.stderr;
      } else {
        const where = got.status === 0 ? undefined : located(got.stderr, dist);
        pass = got.stdout === expected && where === error;
        detail = `--- got\n${got.stdout}${where ? `error ${where}\n` : ""}--- want\n${expected}${error ? `error ${error}\n` : ""}${got.status !== 0 && !error ? got.stderr : ""}`;
      }
      if (!pass) {
        failed++;
        console.error(`FAIL ${fixture} on ${profile.name}/${h} (exit ${got.status})\n${detail}`);
      }
      results.push({ fixture, profile: profile.name, host: h, pass });
    }
  }
}

console.log(`hosts: ${Object.values(versions).join("; ")}`);
for (const r of results) console.log(`${r.pass ? "pass" : "FAIL"}  ${r.fixture.padEnd(15)} ${r.profile.padEnd(12)} ${r.host}`);
console.log(`${results.length - failed} of ${results.length} checks passed`);
if (failed > 0) process.exit(1);
