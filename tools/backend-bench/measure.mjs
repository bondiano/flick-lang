#!/usr/bin/env node
// Repeatable backend cache-miss stage attribution & real-project benchmark (#869 / #456).
//
// Usage:
//   node tools/backend-bench/measure.mjs [--bin <path>] [--samples 5] [--json <out.json>]
//
// Measures wall time and peak RSS (via GNU `time -v`) separately from
// `--timings --diagnostics json` internal stage breakdowns across repeated
// fresh-process runs, while verifying identical outputs and execution stdout.

import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "../..");

const args = process.argv.slice(2);
const argValue = (flag, fallback) => {
  const idx = args.indexOf(flag);
  return idx >= 0 && args[idx + 1] ? args[idx + 1] : fallback;
};

const samples = Number.parseInt(argValue("--samples", "5"), 10);
const jsonOut = argValue("--json", null);
const flickBin = path.resolve(argValue("--bin", path.join(repoRoot, "target/release/flick")));

if (!existsSync(flickBin)) {
  console.error(`error: flick binary not found at ${flickBin}. Run \`cargo build --release -p flick\` first.`);
  process.exit(1);
}

const findGnuTime = () => {
  for (const candidate of ["/usr/bin/time", "/run/current-system/sw/bin/time", "time"]) {
    const res = spawnSync(candidate, ["-v", "true"], { encoding: "utf8" });
    if (res.status === 0 && (res.stderr ?? "").includes("Maximum resident set size")) {
      return candidate;
    }
  }
  return null;
};

const gnuTime = findGnuTime();

const cmdOutput = (cmd, cmdArgs) => {
  try {
    return execFileSync(cmd, cmdArgs, { cwd: repoRoot, encoding: "utf8" }).trim();
  } catch {
    return "unavailable";
  }
};

const sha256File = (filePath) => createHash("sha256").update(readFileSync(filePath)).digest("hex");

const readComponentPin = (name) => {
  const dir = path.join(repoRoot, "backends", name, "package");
  const pin = JSON.parse(readFileSync(path.join(dir, "backend.json"), "utf8"));
  const actual = sha256File(path.join(dir, "component.wasm"));
  if (actual !== pin.sha256) {
    throw new Error(`component ${name} SHA-256 mismatch: expected ${pin.sha256}, got ${actual}`);
  }
  return {
    id: pin.identity.id,
    version: pin.identity.version,
    sha256: pin.sha256,
    wasm_bytes: readFileSync(path.join(dir, "component.wasm")).byteLength,
  };
};

const stats = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  const median = sorted.length % 2 === 1 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
  return {
    median,
    min: sorted[0],
    max: sorted[sorted.length - 1],
    samples: values,
  };
};

const fmtMs = (s) => `${s.median.toFixed(1)} (${s.min.toFixed(1)}–${s.max.toFixed(1)})`;
const fmtMb = (s) => `${(s.median / 1024).toFixed(1)} (${(s.min / 1024).toFixed(1)}–${(s.max / 1024).toFixed(1)})`;

const linkWorkspaceJsPackages = (dir) => {
  const nm = path.join(dir, "node_modules");
  mkdirSync(path.join(nm, "@flick-lang"), { recursive: true });
  symlinkSync(path.join(repoRoot, "runtime"), path.join(nm, "@flick-lang/runtime"));
  symlinkSync(path.join(repoRoot, "packages/effect-ts"), path.join(nm, "@flick-lang/effect-ts"));
  const effectCandidate = [
    path.join(repoRoot, "packages/effect-ts/node_modules/effect"),
    path.join(repoRoot, "node_modules/effect"),
  ].find((p) => existsSync(p));
  if (effectCandidate) {
    symlinkSync(effectCandidate, path.join(nm, "effect"));
  }
};

const runFlick = (cwd, flickArgs, env, measureRss) => {
  const start = process.hrtime.bigint();
  let res;
  if (measureRss && gnuTime) {
    res = spawnSync(gnuTime, ["-v", flickBin, ...flickArgs], {
      cwd,
      env,
      encoding: "utf8",
      maxBuffer: 64 << 20,
    });
  } else {
    res = spawnSync(flickBin, flickArgs, {
      cwd,
      env,
      encoding: "utf8",
      maxBuffer: 64 << 20,
    });
  }
  const wallMs = Number(process.hrtime.bigint() - start) / 1e6;
  if (res.status !== 0) {
    throw new Error(`flick ${flickArgs.join(" ")} failed (${res.status}):\n${res.stderr}\n${res.stdout}`);
  }
  let peakRssKb = null;
  if (measureRss && gnuTime) {
    const match = (res.stderr ?? "").match(/Maximum resident set size \(kbytes\):\s*(\d+)/);
    if (match) peakRssKb = Number.parseInt(match[1], 10);
  }
  let timingsEvent = null;
  if (flickArgs.includes("--timings")) {
    for (const line of (res.stderr ?? "").split("\n")) {
      const trimmed = line.trim();
      if (!trimmed.startsWith("{")) continue;
      try {
        const parsed = JSON.parse(trimmed);
        if (parsed.type === "timings") timingsEvent = parsed;
      } catch {
        // ignore non-JSON lines
      }
    }
  }
  return { wallMs, peakRssKb, timingsEvent };
};

const hashDirectory = (dir) => {
  const entries = [];
  const walk = (cur, rel) => {
    for (const item of execFileSync("ls", ["-A", cur], { encoding: "utf8" }).split("\n").filter(Boolean).sort()) {
      const full = path.join(cur, item);
      const nextRel = rel ? `${rel}/${item}` : item;
      const st = spawnSync("test", ["-d", full]);
      if (st.status === 0) {
        walk(full, nextRel);
      } else {
        entries.push(`${nextRel}:${sha256File(full)}`);
      }
    }
  };
  walk(dir, "");
  return createHash("sha256").update(entries.join("\n")).digest("hex");
};

const extractBreakdown = (event) => {
  const stages = Object.fromEntries((event?.stages ?? []).map((s) => [s.label, s.micros / 1000]));
  const phases = {};
  for (const p of event?.phases ?? []) {
    phases[p.label] = (phases[p.label] ?? 0) + p.micros / 1000;
  }
  const metrics = Object.fromEntries((event?.metrics ?? []).map((m) => [m.label, m.bytes]));
  return { stages, phases, metrics };
};

const aggregateBreakdowns = (list) => {
  const stageKeys = new Set();
  const phaseKeys = new Set();
  const metricKeys = new Set();
  for (const item of list) {
    for (const k of Object.keys(item.stages)) stageKeys.add(k);
    for (const k of Object.keys(item.phases)) phaseKeys.add(k);
    for (const k of Object.keys(item.metrics)) metricKeys.add(k);
  }
  const stages = {};
  for (const k of stageKeys) stages[k] = stats(list.map((i) => i.stages[k] ?? 0));
  const phases = {};
  for (const k of phaseKeys) phases[k] = stats(list.map((i) => i.phases[k] ?? 0));
  const metrics = {};
  for (const k of metricKeys) metrics[k] = stats(list.map((i) => i.metrics[k] ?? 0));
  return { stages, phases, metrics };
};

const setupMtl = (root, runtime) => {
  const dir = path.join(root, `mtl-${runtime}`);
  cpSync(path.join(repoRoot, "examples/mtl"), dir, { recursive: true });
  rmSync(path.join(dir, ".flick"), { recursive: true, force: true });
  rmSync(path.join(dir, "dist"), { recursive: true, force: true });
  linkWorkspaceJsPackages(dir);
  let toml = readFileSync(path.join(dir, "flick.toml"), "utf8");
  toml = toml.replace(/runtime\s*=\s*"[^"]+"/, `runtime = "${runtime}"`);
  writeFileSync(path.join(dir, "flick.toml"), toml);
  return {
    dir,
    touchFile: path.join(dir, "src/Mtl/StateT.fl"),
    buildArgs: ["build"],
    runCmd: ["node", [path.join(dir, "dist/flick-main.js"), "catalog"]],
  };
};

const SMALL_SOURCE = `module Main exposing (sum, main)

sum : Int -> Int -> Int
let rec sum acc n =
  if n <= 0 then acc else sum (acc + n) (n - 1)

main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (show (sum 0 100))
`;

const setupSmall = (root, target, runtime) => {
  const label = target === "go" ? "small-go" : `small-js-${runtime}`;
  const dir = path.join(root, label);
  mkdirSync(path.join(dir, "src"), { recursive: true });
  writeFileSync(path.join(dir, "src/Main.fl"), SMALL_SOURCE);
  const toml =
    target === "go"
      ? `schema-version = 1

[project]
kind = "app"
entry = "Main"
source-dir = "src"

[build]
out-dir = "dist"
profile = "release"
target = "go"

[tools]
"flick/go" = ["go"]
`
      : `schema-version = 1

[project]
kind = "app"
entry = "Main"
source-dir = "src"

[build]
out-dir = "dist"
profile = "release"
target = "js"

[target.js]
runtime = "${runtime}"
`;
  writeFileSync(path.join(dir, "flick.toml"), toml);
  if (target === "js") {
    writeFileSync(
      path.join(dir, "package.json"),
      JSON.stringify(
        {
          name: label,
          private: true,
          type: "module",
          dependencies: {
            "@flick-lang/runtime": "0.2.0",
            "@flick-lang/effect-ts": "0.2.0",
          },
        },
        null,
        2,
      ) + "\n",
    );
    linkWorkspaceJsPackages(dir);
  }
  return {
    dir,
    touchFile: path.join(dir, "src/Main.fl"),
    buildArgs: ["build"],
    runCmd:
      target === "go"
        ? [path.join(dir, "dist/flick-main"), []]
        : ["node", [path.join(dir, "dist/flick-main.js")]],
  };
};

const measureScenario = (name, setup, baseEnv) => {
  const originalSource = readFileSync(setup.touchFile, "utf8");
  const env = { ...baseEnv };

  // Warmup build: populates Wasmtime compiled-code cache, std module cache, and build cache.
  runFlick(setup.dir, setup.buildArgs, env, false);
  const expectedStdout = execFileSync(setup.runCmd[0], setup.runCmd[1], {
    cwd: setup.dir,
    encoding: "utf8",
  }).trim();
  const baselineHash = hashDirectory(path.join(setup.dir, "dist"));

  // 1. Cache HIT (wall + RSS without --timings)
  const hitWall = [];
  const hitRss = [];
  for (let i = 0; i < samples; i++) {
    const r = runFlick(setup.dir, setup.buildArgs, env, true);
    hitWall.push(r.wallMs);
    if (r.peakRssKb !== null) hitRss.push(r.peakRssKb);
  }
  // 2. Cache HIT (--timings --diagnostics json)
  const hitTimings = [];
  for (let i = 0; i < samples; i++) {
    const r = runFlick(setup.dir, [...setup.buildArgs, "--timings", "--diagnostics", "json"], env, false);
    hitTimings.push(extractBreakdown(r.timingsEvent));
  }

  // 3. Forced source MISS (wall + RSS without --timings)
  const missWall = [];
  const missRss = [];
  for (let i = 0; i < samples; i++) {
    writeFileSync(setup.touchFile, `${originalSource}\n// bench-miss-wall-${i}\n`);
    const r = runFlick(setup.dir, setup.buildArgs, env, true);
    missWall.push(r.wallMs);
    if (r.peakRssKb !== null) missRss.push(r.peakRssKb);
  }
  // 4. Forced source MISS (--timings --diagnostics json)
  const missTimings = [];
  for (let i = 0; i < samples; i++) {
    writeFileSync(setup.touchFile, `${originalSource}\n// bench-miss-timings-${i}\n`);
    const r = runFlick(setup.dir, [...setup.buildArgs, "--timings", "--diagnostics", "json"], env, false);
    missTimings.push(extractBreakdown(r.timingsEvent));
  }

  // Verify matched-input artifact determinism and execution output after restoring source.
  writeFileSync(setup.touchFile, originalSource);
  runFlick(setup.dir, setup.buildArgs, env, false);
  const restoredHash = hashDirectory(path.join(setup.dir, "dist"));
  const restoredStdout = execFileSync(setup.runCmd[0], setup.runCmd[1], {
    cwd: setup.dir,
    encoding: "utf8",
  }).trim();
  if (restoredStdout !== expectedStdout) {
    throw new Error(`${name}: execution stdout changed (${expectedStdout} vs ${restoredStdout})`);
  }
  if (restoredHash !== baselineHash) {
    throw new Error(`${name}: dist artifacts differ on matched source (${baselineHash} vs ${restoredHash})`);
  }

  return {
    name,
    stdout: expectedStdout,
    dist_sha256: baselineHash,
    hit: {
      wall_ms: stats(hitWall),
      peak_rss_kb: hitRss.length > 0 ? stats(hitRss) : null,
      breakdown: aggregateBreakdowns(hitTimings),
    },
    miss: {
      wall_ms: stats(missWall),
      peak_rss_kb: missRss.length > 0 ? stats(missRss) : null,
      breakdown: aggregateBreakdowns(missTimings),
    },
  };
};

const tmpRoot = mkdtempSync(path.join(os.tmpdir(), "flick-backend-bench-"));
const wasmtimeCache = path.join(tmpRoot, "wasmtime-cache");
const xdgCache = path.join(tmpRoot, "xdg-cache");
mkdirSync(wasmtimeCache, { recursive: true });
mkdirSync(xdgCache, { recursive: true });

const baseEnv = {
  ...process.env,
  FLICK_STD_PATH: path.join(repoRoot, "packages/std"),
  FLICK_WASMTIME_CACHE_DIR: wasmtimeCache,
  XDG_CACHE_HOME: xdgCache,
};

const envInfo = {
  date_utc: new Date().toISOString(),
  os: `${os.type()} ${os.release()} ${os.arch()}`,
  cpu: os.cpus()[0]?.model ?? "unknown",
  cpu_cores: os.cpus().length,
  mem_gb: Number((os.totalmem() / (1024 ** 3)).toFixed(1)),
  git_sha: cmdOutput("git", ["rev-parse", "HEAD"]),
  rustc: cmdOutput("rustc", ["--version"]),
  node: cmdOutput("node", ["--version"]),
  go: cmdOutput("go", ["version"]),
  gnu_time: gnuTime ?? "unavailable",
  flick_bin: flickBin,
  samples,
  components: {
    js: readComponentPin("js"),
    go: readComponentPin("go"),
  },
  cache_state: {
    wasmtime_cache: "warm (shared compiled-code cache populated during scenario warmup)",
    module_store: "warm (isolated XDG_CACHE_HOME per benchmark run)",
    build_cache: "hit = unchanged project; miss = unique comment appended to source file",
  },
};

try {
  const results = [
    measureScenario("mtl / JS RT", setupMtl(tmpRoot, "rt"), baseEnv),
    measureScenario("mtl / JS Effect", setupMtl(tmpRoot, "effect"), baseEnv),
    measureScenario("small / JS RT", setupSmall(tmpRoot, "js", "rt"), baseEnv),
    measureScenario("small / JS Effect", setupSmall(tmpRoot, "js", "effect"), baseEnv),
    measureScenario("small / Go control", setupSmall(tmpRoot, "go", null), baseEnv),
  ];

  // Cross-check identical execution stdout across runtimes/targets for matched programs.
  if (results[0].stdout !== results[1].stdout) {
    throw new Error(`mtl stdout mismatch between JS RT and JS Effect`);
  }
  if (results[2].stdout !== results[3].stdout || results[2].stdout !== results[4].stdout) {
    throw new Error(`small stdout mismatch across JS RT / JS Effect / Go`);
  }

  const report = { environment: envInfo, scenarios: results };
  if (jsonOut) {
    writeFileSync(jsonOut, JSON.stringify(report, null, 2) + "\n");
  }

  console.log("## Pinned Environment");
  console.log(`- OS / CPU: ${envInfo.os} — ${envInfo.cpu} (${envInfo.cpu_cores} cores, ${envInfo.mem_gb} GiB RAM)`);
  console.log(`- Toolchains: ${envInfo.rustc} | Node ${envInfo.node} | ${envInfo.go}`);
  console.log(`- CLI git SHA: \`${envInfo.git_sha}\` (\`${path.relative(repoRoot, flickBin)}\`)`);
  console.log(`- Official JS component: \`${envInfo.components.js.id}@${envInfo.components.js.version}\` sha256 \`${envInfo.components.js.sha256}\` (${envInfo.components.js.wasm_bytes} bytes)`);
  console.log(`- Official Go component: \`${envInfo.components.go.id}@${envInfo.components.go.version}\` sha256 \`${envInfo.components.go.sha256}\` (${envInfo.components.go.wasm_bytes} bytes)`);
  console.log(`- Samples per mode: ${samples} fresh processes (reporting median (min–max))`);
  console.log("");

  console.log("## Process Wall Time & Peak RSS (uninstrumented, no `--timings`)");
  console.log("| Scenario | Hit Wall (ms) | Hit Peak RSS (MiB) | Miss Wall (ms) | Miss Peak RSS (MiB) | Miss − Hit (ms) |");
  console.log("| --- | ---: | ---: | ---: | ---: | ---: |");
  for (const s of results) {
    const delta = s.miss.wall_ms.median - s.hit.wall_ms.median;
    console.log(
      `| ${s.name} | ${fmtMs(s.hit.wall_ms)} | ${s.hit.peak_rss_kb ? fmtMb(s.hit.peak_rss_kb) : "n/a"} | ${fmtMs(s.miss.wall_ms)} | ${s.miss.peak_rss_kb ? fmtMb(s.miss.peak_rss_kb) : "n/a"} | +${delta.toFixed(1)} |`,
    );
  }
  console.log("");

  const orderedPhases = [
    "backend component init (wasmtime load)",
    "backend component tools discovery",
    "backend preflight & descriptor",
    "backend prepare request",
    "backend prepare: preflight & prune",
    "backend prepare: program build",
    "backend prepare: program CBOR encode",
    "backend prepare: projections & inputs",
    "backend runtime dependencies",
    "backend handshake",
    "backend host input snapshot",
    "backend host request validation (decode #1)",
    "backend check-target (guest decode #2)",
    "backend generate #1 (guest decode #3)",
    "backend result validation #1",
    "backend determinism replay generate #2 (guest decode #4)",
    "backend replay validation #2",
    "backend cache key",
    "backend external tool steps",
    "backend emission residual",
    "atomic output publication",
  ];

  console.log("## Forced Source Miss — Internal Stage Attribution (`--timings --diagnostics json`, ms)");
  for (const s of results) {
    const b = s.miss.breakdown;
    const progB = b.metrics.program_bytes?.median ?? 0;
    const inB = b.metrics.inputs_bytes?.median ?? 0;
    console.log(`\n### ${s.name} (program_bytes=${progB}, inputs_bytes=${inB})`);
    console.log("| Stage / Sub-stage | Median (min–max) ms |");
    console.log("| --- | ---: |");
    for (const [label, st] of Object.entries(b.stages)) {
      console.log(`| **[stage] ${label}** | ${fmtMs(st)} |`);
    }
    for (const label of orderedPhases) {
      if (b.phases[label]) {
        console.log(`| ${label} | ${fmtMs(b.phases[label])} |`);
      }
    }
  }
} finally {
  rmSync(tmpRoot, { recursive: true, force: true });
}
