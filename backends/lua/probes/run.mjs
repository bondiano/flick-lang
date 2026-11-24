// Runs the Lua profile probes on the pinned hosts and compares each output
// with its independently written expectation; checks the executable API
// snapshots; writes the WoW probe addons, which no host here can run.
//
// usage: node backends/lua/probes/run.mjs [--write-api]
// Hosts come from PATH or LUA51, LUAJIT, LUAC and LOVE. A host of another
// version fails the run: no probe falls back to a different VM.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const probes = dirname(fileURLToPath(import.meta.url));
const lua = resolve(probes, "..");
const root = resolve(lua, "../..");
const out = join(root, "target/lua-probes");
const writeApi = process.argv.includes("--write-api");
const host = (name, fallback) => process.env[name] || fallback;
const [lua51, luajit, luac, love] = [host("LUA51", "lua"), host("LUAJIT", "luajit"), host("LUAC", "luac"), host("LOVE", "love")];
// LÖVE without a display or an audio device
const headless = { ...process.env, SDL_VIDEODRIVER: "dummy", SDL_AUDIODRIVER: "dummy", ALSOFT_DRIVERS: "null" };

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
  luajit: version(luajit, ["-v"], /^LuaJIT 2\.1\./),
  love: version(love, ["--version"], /^LOVE 11\.5 /),
};

// one game, a host callback after the probe: love.load runs it, the first
// love.update reports itself and quits
const game = join(out, "love");
rmSync(game, { recursive: true, force: true });
mkdirSync(game, { recursive: true });
writeFileSync(join(game, "conf.lua"), "function love.conf(t)\n  t.window = false\nend\n");
writeFileSync(join(game, "main.lua"), `function love.load(args)
  local ok, err = pcall(love.filesystem.load(args[1]))
  if not ok then print("probe failed: " .. tostring(err)); love.event.quit(1) end
end
function love.update(dt)
  print("love.update", type(dt))
  love.event.quit(0)
end
`);
const callback = "love.update\tnumber\n";

const hosts = [
  { name: "lua51", keys: ["lua51"], run: (file) => run(lua51, [file], { cwd: probes }) },
  { name: "luajit", keys: ["luajit"], run: (file) => run(luajit, [file], { cwd: probes }) },
  { name: "luajit-joff", keys: ["luajit-joff", "luajit"], run: (file) => run(luajit, ["-joff", file], { cwd: probes }) },
  { name: "love", keys: ["love", "luajit"], run: (file) => run(love, [game, file], { env: headless }), love: true },
];

const names = readdirSync(probes).filter((f) => f.endsWith(".lua")).map((f) => f.slice(0, -4)).sort();
for (const name of names) copyFileSync(join(probes, `${name}.lua`), join(game, `${name}.lua`));
copyFileSync(join(lua, "profiles/api/dump.lua"), join(game, "dump.lua"));

function expected(name, keys) {
  for (const key of [...keys.map((k) => `${name}.${k}`), name]) {
    const file = join(probes, `${key}.expected`);
    if (existsSync(file)) return { file: `${key}.expected`, text: readFileSync(file, "utf8") };
  }
  throw new Error(`no expectation for ${name}`);
}

const results = [];
let failed = 0;
for (const name of names) {
  for (const h of hosts) {
    const want = expected(name, h.keys);
    const got = h.run(`${name}.lua`);
    let stdout = got.stdout;
    let pass = got.status === 0;
    if (h.love) {
      pass &&= stdout.endsWith(callback);
      stdout = stdout.slice(0, stdout.length - callback.length);
    }
    pass &&= stdout === want.text;
    if (!pass) {
      failed++;
      console.error(`FAIL ${name} on ${h.name} (expected ${want.file}, exit ${got.status})\n--- got\n${got.stdout}${got.stderr}--- want\n${want.text}`);
    }
    results.push({ probe: name, host: h.name, expected: want.file, pass });
  }
}

// the executable API snapshots
const snapshots = [
  ["lua-5.1.txt", () => run(lua51, ["dump.lua"], { cwd: join(lua, "profiles/api") })],
  ["luajit-2.1.txt", () => run(luajit, ["dump.lua"], { cwd: join(lua, "profiles/api") })],
  ["love-11.5.txt", () => run(love, [game, "dump.lua"], { env: headless })],
];
for (const [file, dump] of snapshots) {
  const got = dump();
  const text = got.stdout.replace(callback, "");
  const path = join(lua, "profiles/api", file);
  if (got.status !== 0) throw new Error(`${file}: ${got.stderr}`);
  if (writeApi) writeFileSync(path, text);
  const pass = existsSync(path) && readFileSync(path, "utf8") === text;
  if (!pass) {
    failed++;
    console.error(`FAIL the API snapshot ${file} differs from its host; rerun with --write-api after review`);
  }
  results.push({ probe: `api ${file}`, host: file.replace(/\.txt$/, ""), pass });
}

// the WoW probe addons: parsed by Lua 5.1, not executed
for (const profile of ["wow-retail", "wow-forever"]) {
  const toml = readFileSync(join(lua, `profiles/${profile}.toml`), "utf8");
  const interfaceNumber = /^interface = (\d+)$/m.exec(toml)?.[1];
  const build = /^build = "([^"]+)"$/m.exec(toml)?.[1];
  if (!interfaceNumber || !build) throw new Error(`profiles/${profile}.toml names no interface and build`);
  const addon = join(out, "wow", profile, "FlickProbes");
  rmSync(addon, { recursive: true, force: true });
  mkdirSync(addon, { recursive: true });
  for (const name of names) copyFileSync(join(probes, `${name}.lua`), join(addon, `${name}.lua`));
  writeFileSync(join(addon, "FlickProbes.lua"), `-- the host callback: ADDON_LOADED of this addon
local name = ...
local frame = CreateFrame("Frame")
frame:RegisterEvent("ADDON_LOADED")
frame:SetScript("OnEvent", function(_, event, loaded)
  if loaded == name then print(event, loaded) end
end)
`);
  const files = [...names.map((n) => `${n}.lua`), "FlickProbes.lua"];
  writeFileSync(join(addon, "FlickProbes.toc"), [
    `## Interface: ${interfaceNumber}`,
    "## Title: Flick Lua profile probes",
    `## Notes: Generated for ${profile} ${build}; not run by CI`,
    ...files,
    "",
  ].join("\n"));
  const parsed = run(luac, ["-p", ...files], { cwd: addon });
  const pass = parsed.status === 0;
  if (!pass) {
    failed++;
    console.error(`FAIL the ${profile} addon does not parse:\n${parsed.stderr}`);
  }
  results.push({ probe: "addon", host: profile, pass, executed: false });
}

const report = { date: new Date().toISOString().slice(0, 10), versions, results };
writeFileSync(join(out, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
for (const r of results) console.log(`${r.pass ? "ok  " : "FAIL"} ${r.probe.padEnd(24)} ${r.host}${r.executed === false ? " (generated, not executed)" : ""}`);
console.log(`${Object.values(versions).join("; ")}\n${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
