#!/usr/bin/env node
// Node.js verification harness for the wasm32 build of `flick-vm` (`flick-vm-wasm`).
//
// Verifies:
// 1. `tests/language` corpus parity between `flick_vm_wasm.wasm` and native
//    Tier 0 (`flick test --target vm`), with an explicit documented allowlist
//    for host-only OS cases (none currently needed in `tests/language`).
// 2. Cooperative single-threaded stepping (issue #997 AC 3): an infinite loop
//    with LIFO `Scope.add_finalizer` hooks yields control back to JS after each
//    `step(fuel)` slice, unwinds finalizers cleanly on `abandon()`, leaks zero
//    heap bytes, and leaves the same `VmSession` healthy for subsequent evals.
// 3. JS host callbacks (`writeStdout`, `currentTimeMillis`, `randomNext`,
//    `envVar`, `fetch` capability gating), `wallTimeMs` backed by JS
//    `performance.now()`, and `.fvm` byte-buffer `save_image` / `load_image`
//    round-trips across fresh sessions.
//
// Usage:
//   node tools/vm-wasm/index.mjs [--flick <bin>] [--wasm <path>] [--wasm-bindgen <bin>]

import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createRequire } from "node:module";
import { availableParallelism, tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import {
  findSingleFvm,
  verifyCooperativeSteppingAndAbandon,
  verifyHostCallbacksLimitsAndImages,
} from "./checks.mjs";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const require = createRequire(import.meta.url);

// Explicit, documented host-only allowlist for tests in `tests/language` that
// require OS-only capabilities unavailable on wasm32 (filesystem, child
// processes, native threads, OS TZif files). Must stay empty unless a new
// OS-bound suite is added to `tests/language`.
const HOST_ONLY_ALLOWLIST = new Map([
  // Example: ["Language.FsSuite > reads temp file", "requires native Host.FileSystem"]
]);

function parseArgs(argv) {
  const opts = {
    flick: null,
    wasm: null,
    wasmBindgen: null,
    seed: 42,
    skipCorpus: false,
  };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--flick") {
      opts.flick = argv[++i];
    } else if (arg === "--wasm") {
      opts.wasm = argv[++i];
    } else if (arg === "--wasm-bindgen") {
      opts.wasmBindgen = argv[++i];
    } else if (arg === "--seed") {
      opts.seed = Number.parseInt(argv[++i], 10);
    } else if (arg === "--skip-corpus") {
      opts.skipCorpus = true;
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  return opts;
}

function runChecked(cmd, args, options = {}) {
  const result = spawnSync(cmd, args, {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    ...options,
  });
  if (result.error) {
    throw new Error(`failed to spawn ${cmd}: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new Error(
      `${cmd} ${args.join(" ")} exited with ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
  }
  return result;
}

function runAsync(cmd, args, options = {}) {
  return new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(cmd, args, options);
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", (err) => {
      rejectPromise(new Error(`failed to spawn ${cmd}: ${err.message}`));
    });
    child.on("close", (code) => {
      if (code !== 0) {
        rejectPromise(
          new Error(
            `${cmd} ${args.join(" ")} exited with ${code}\nstdout:\n${stdout}\nstderr:\n${stderr}`,
          ),
        );
      } else {
        resolvePromise({ stdout, stderr });
      }
    });
  });
}

function resolveTargetDir() {
  return process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(repoRoot, "target");
}

function resolveFlickBin(explicit) {
  if (explicit) {
    return path.resolve(explicit);
  }
  const targetDir = resolveTargetDir();
  const candidates = [
    path.join(targetDir, "e2e", "flick"),
    path.join(targetDir, "release", "flick"),
    path.join(repoRoot, "target", "e2e", "flick"),
    path.join(repoRoot, "target", "release", "flick"),
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  runChecked("cargo", ["build", "-p", "flick", "--profile", "e2e"], { cwd: repoRoot });
  return path.join(targetDir, "e2e", "flick");
}

function resolveWasmFile(explicit) {
  if (explicit) {
    return path.resolve(explicit);
  }
  const targetDir = resolveTargetDir();
  const candidates = [
    path.join(targetDir, "wasm32-unknown-unknown", "release", "flick_vm_wasm.wasm"),
    path.join(repoRoot, "target", "wasm32-unknown-unknown", "release", "flick_vm_wasm.wasm"),
  ];
  for (const candidate of candidates) {
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  throw new Error(
    `flick_vm_wasm.wasm not found (checked ${candidates.join(", ")}); run \`just wasm\` first`,
  );
}

function resolveWasmBindgen(explicit) {
  if (explicit) {
    return path.resolve(explicit);
  }
  const localTools = path.join(repoRoot, "target", "tools", "bin", "wasm-bindgen");
  if (existsSync(localTools)) {
    return localTools;
  }
  return "wasm-bindgen";
}

function normalizeCaseStatus(rawCase) {
  if (rawCase.skipped) {
    return "skipped";
  }
  return rawCase.passed ? "passed" : "failed";
}

async function verifyLanguageCorpus(vmWasm, flickBin, workDir, seed, env) {
  const languageDir = path.join(repoRoot, "tests", "language");

  // 1. Run native Tier 0 baseline.
  const nativeRun = runChecked(
    flickBin,
    ["test", "--target", "vm", "--seed", String(seed), "--reporter", "json"],
    { cwd: languageDir, env },
  );
  const nativeReport = JSON.parse(nativeRun.stdout);
  assert.ok(Array.isArray(nativeReport.tests), "native json report must include tests array");
  const nativeByName = new Map();
  for (const item of nativeReport.tests) {
    nativeByName.set(item.name, item.status);
  }
  assert.ok(nativeByName.size > 0, "native language suite must execute at least one test");

  // 2. Partition tests/language/test/Language/*.fl into batches of 4 suites so
  // each emitted .fvm image stays well within #992's 2M CBOR verifier budget.
  const testDir = path.join(languageDir, "test", "Language");
  const suiteFiles = readdirSync(testDir)
    .filter((name) => name.endsWith(".fl"))
    .sort();
  assert.ok(suiteFiles.length >= 30, `expected >= 30 Language test files, got ${suiteFiles.length}`);

  const batchSize = 4;
  const batches = [];
  for (let i = 0; i < suiteFiles.length; i += batchSize) {
    batches.push(suiteFiles.slice(i, i + batchSize));
  }

  const jobs = Math.max(1, Math.min(availableParallelism(), 6));
  const fvmImages = new Array(batches.length);
  let nextBatch = 0;

  async function buildWorker() {
    while (nextBatch < batches.length) {
      const idx = nextBatch++;
      const files = batches[idx];
      const stageDir = path.join(workDir, `language-batch-${idx}`);
      const stageSrc = path.join(stageDir, "src", "Language");
      mkdirSync(stageSrc, { recursive: true });
      writeFileSync(
        path.join(stageDir, "flick.toml"),
        [
          "schema-version = 1",
          "",
          "[project]",
          'kind = "lib"',
          'source-dir = "src"',
          "",
          "[build]",
          'out-dir = "dist"',
          'profile = "debug"',
          'target = "vm"',
          "",
        ].join("\n"),
      );
      cpSync(path.join(languageDir, "src", "Language"), stageSrc, { recursive: true });
      for (const file of files) {
        cpSync(path.join(testDir, file), path.join(stageSrc, file));
      }
      await runAsync(flickBin, ["build", "--target", "vm", "--out-dir", "dist"], {
        cwd: stageDir,
        env,
      });
      fvmImages[idx] = readFileSync(findSingleFvm(path.join(stageDir, "dist")));
    }
  }
  await Promise.all(Array.from({ length: jobs }, buildWorker));

  // 3. Load each batch .fvm into flick-vm-wasm and execute every Language.<Suite>.tests global.
  const wasmByName = new Map();
  const wasmFailures = [];
  let suiteCount = 0;
  for (const fvmBytes of fvmImages) {
    const session = new vmWasm.VmSession(
      JSON.stringify({ preset: "unrestricted", hostMode: "live" }),
    );
    const loadReport = JSON.parse(session.load_image(fvmBytes));
    const testGlobals = loadReport.loadedGlobals
      .filter((g) => g.name === "tests" && g.module.startsWith("Language."))
      .sort((a, b) => a.module.localeCompare(b.module));
    for (const entry of testGlobals) {
      suiteCount++;
      const rawCases = JSON.parse(
        session.run_loaded_test_suite(entry.module, entry.name, seed, 0, undefined),
      );
      for (const tc of rawCases) {
        const status = normalizeCaseStatus(tc);
        wasmByName.set(tc.name, status);
        if (status === "failed") {
          wasmFailures.push(`${entry.module} :: ${tc.name}: ${tc.message ?? "(no message)"}`);
        }
      }
    }
    session.shutdown();
    session.free();
  }
  assert.ok(suiteCount >= 30, `expected >= 30 Language test suites, got ${suiteCount}`);

  // 4. Compare wasm32 results against native Tier 0.
  const mismatches = [];
  const usedAllowlist = new Set();
  for (const [name, nativeStatus] of nativeByName.entries()) {
    const wasmStatus = wasmByName.get(name);
    if (HOST_ONLY_ALLOWLIST.has(name)) {
      usedAllowlist.add(name);
      continue;
    }
    if (wasmStatus !== nativeStatus) {
      mismatches.push(`${name}: native=${nativeStatus} wasm32=${wasmStatus ?? "missing"}`);
    }
  }
  for (const name of wasmByName.keys()) {
    if (!nativeByName.has(name)) {
      mismatches.push(`${name}: present in wasm32 but missing from native Tier 0`);
    }
  }
  for (const allowed of HOST_ONLY_ALLOWLIST.keys()) {
    if (!usedAllowlist.has(allowed)) {
      mismatches.push(`stale HOST_ONLY_ALLOWLIST entry: ${allowed}`);
    }
  }
  assert.equal(
    mismatches.length,
    0,
    `wasm32 vs Tier 0 language corpus mismatches:\n${mismatches.join("\n")}\nfailures:\n${wasmFailures.join("\n")}`,
  );
  assert.equal(wasmFailures.length, 0, `unexpected wasm32 failures:\n${wasmFailures.join("\n")}`);

  console.log(
    `vm-wasm language corpus: ${wasmByName.size}/${nativeByName.size} cases matched Tier 0 across ${suiteCount} suites (allowlisted host-only: ${HOST_ONLY_ALLOWLIST.size})`,
  );
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  const flickBin = resolveFlickBin(opts.flick);
  const wasmFile = resolveWasmFile(opts.wasm);
  const wasmBindgen = resolveWasmBindgen(opts.wasmBindgen);
  const env = {
    ...process.env,
    FLICK_STD_PATH: process.env.FLICK_STD_PATH || path.join(repoRoot, "packages", "std"),
  };

  const workDir = mkdtempSync(path.join(tmpdir(), "flick-vm-wasm-"));
  try {
    const pkgDir = path.join(workDir, "pkg");
    runChecked(wasmBindgen, ["--target", "nodejs", "--out-dir", pkgDir, wasmFile]);
    const vmWasm = require(path.join(pkgDir, "flick_vm_wasm.js"));

    assert.ok(typeof vmWasm.vm_version() === "string" && vmWasm.vm_version().length > 0);
    assert.equal(vmWasm.image_compiler_hash().length, 64);
    assert.equal(vmWasm.image_memory_ir_hash().length, 64);

    if (!opts.skipCorpus) {
      await verifyLanguageCorpus(vmWasm, flickBin, workDir, opts.seed, env);
    }
    verifyCooperativeSteppingAndAbandon(runChecked, vmWasm, flickBin, workDir, env);
    verifyHostCallbacksLimitsAndImages(runChecked, vmWasm, flickBin, workDir, env);
  } finally {
    rmSync(workDir, { recursive: true, force: true });
  }
}

await main();
