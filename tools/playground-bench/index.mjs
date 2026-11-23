// An initialized real compiler.worker.ts in Chromium, without building the site.
// node tools/playground-bench/index.mjs <wasm-bindgen web directory> [report.json]
// SAMPLES=20 WARMUP=3 CHROMIUM=/path/to/chromium; WORKER_SOURCE pins an older worker.
// ENGINE=node runs the same worker in Node/V8 when Chromium is unavailable.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { createRequire, stripTypeScriptTypes } from "node:module";
import { cpus, platform, arch } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { Worker as NodeWorker } from "node:worker_threads";
import { gzipSync } from "node:zlib";

const root = fileURLToPath(new URL("../..", import.meta.url));
const wasmDirectory = path.resolve(process.argv[2] ?? "docs/src/playground/wasm");
const output = process.argv[3];
const samples = Number(process.env.SAMPLES ?? 20);
const warmup = Number(process.env.WARMUP ?? 3);
assert.ok(Number.isInteger(samples) && samples > 0, "SAMPLES must be positive");
assert.ok(Number.isInteger(warmup) && warmup >= 0, "WARMUP must be nonnegative");
const engine = process.env.ENGINE ?? "chromium";
assert.ok(["chromium", "node"].includes(engine), "ENGINE must be chromium or node");
const workerPath = process.env.WORKER_SOURCE ?? path.join(root, "docs/src/playground/compiler.worker.ts");
const workerSource = readFileSync(workerPath, "utf8");
let worker = stripTypeScriptTypes(workerSource);
const wasmUrl = pathToFileURL(path.join(wasmDirectory, "flick_wasm.js")).href;
if (engine === "node") worker = worker.replace('"./wasm/flick_wasm.js"', JSON.stringify(wasmUrl));
const wasmBytes = readFileSync(path.join(wasmDirectory, "flick_wasm_bg.wasm"));
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const fixtureNames = (process.env.FIXTURES ?? "hello,classes,sales-report").split(",");
const fixtures = fixtureNames.map((name) => {
  assert.match(name, /^[a-z-]+$/);
  const source = readFileSync(path.join(root, `docs/src/playground/examples/${name}.fl`), "utf8");
  return { name, source, sha256: hash(source) };
});

// The production worker still handles every request. This wrapper observes
// exported WASM calls and its reply; it does not implement a mock compiler.
const preamble = engine === "node" ? `
import { parentPort } from "node:worker_threads";
import { readFileSync } from "node:fs";
import init, { Session } from ${JSON.stringify(wasmUrl)};
globalThis.onmessage = null;
globalThis.onmessageerror = null;
globalThis.postMessage = (message) => parentPort.postMessage(message);
parentPort.on("message", (data) => globalThis.onmessage({ data }));
const wasm = await init({ module_or_path: readFileSync(${JSON.stringify(path.join(wasmDirectory, "flick_wasm_bg.wasm"))}) });
` : `import init, { Session } from "/wasm/flick_wasm.js"; const wasm = await init();`;
const workerUrl = engine === "node" ? `data:text/javascript;base64,${Buffer.from(worker).toString("base64")}` : "/compiler.worker.js";
const wrapper = `${preamble}
let calls = [];
let stages = [];
let stack = [];
let started = 0;
// Only the optional stages.patch build imports this function. Production
// builds have no stage hooks and report an empty stages array.
globalThis.__flickBenchMark = (id, enter) => {
  const now = performance.now();
  if (enter) stack.push({ id, at: now, children: 0 });
  else {
    const stage = stack.pop();
    if (stage?.id !== id) throw new Error("unbalanced compiler stage");
    const inclusiveMs = now - stage.at;
    stages.push({ id, inclusiveMs, exclusiveMs: inclusiveMs - stage.children });
    if (stack.length) stack[stack.length - 1].children += inclusiveMs;
  }
};
for (const name of ["update", "prepare_emit", "emit", "emit_declarations"]) {
  const original = Session.prototype[name];
  if (!original) continue;
  Session.prototype[name] = function (...args) {
    const at = performance.now();
    const result = original.apply(this, args);
    calls.push({ name, ms: performance.now() - at });
    return result;
  };
}
const send = globalThis.postMessage.bind(globalThis);
globalThis.postMessage = (message) => send({ ...message, measurement: {
  workerMs: performance.now() - started,
  calls,
  stages,
  linearMemoryBytes: wasm.memory.buffer.byteLength,
} });
await import(${JSON.stringify(workerUrl)});
const handle = globalThis.onmessage;
globalThis.onmessage = (event) => {
  calls = [];
  stages = [];
  stack = [];
  started = performance.now();
  return handle(event);
};
send({ ready: true });
`;
const routes = new Map([
  ["/", ["text/html", "<!doctype html><title>Playground compiler benchmark</title>"]],
  ["/probe.worker.js", ["text/javascript", wrapper]],
  ["/compiler.worker.js", ["text/javascript", worker]],
  ["/wasm/flick_wasm.js", ["text/javascript", readFileSync(path.join(wasmDirectory, "flick_wasm.js"))]],
  ["/wasm/flick_wasm_bg.wasm", ["application/wasm", wasmBytes]],
]);
const server = createServer((request, response) => {
  const route = routes.get(new URL(request.url, "http://localhost").pathname);
  if (!route) { response.writeHead(404); response.end(); return; }
  response.writeHead(200, { "Content-Type": route[0], "Cache-Control": "no-store" });
  response.end(route[1]);
});
let browser;
let browserVersion;
try {
  let evaluate;
  if (engine === "node") {
    globalThis.Worker = class {
      constructor() {
        this.worker = new NodeWorker(new URL(`data:text/javascript;base64,${Buffer.from(wrapper).toString("base64")}`));
        this.worker.on("message", (data) => this.onmessage?.({ data }));
        this.worker.on("error", (error) => this.onerror?.({ message: error.message }));
      }
      postMessage(message) { this.worker.postMessage(message); }
      terminate() { return this.worker.terminate(); }
    };
    evaluate = (action, input) => action(input);
  } else {
    const require = createRequire(new URL("../../docs/package.json", import.meta.url));
    const { chromium } = require(process.env.PLAYWRIGHT_MODULE ?? "@playwright/test");
    await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
    browser = await chromium.launch({
      headless: true,
      ...(process.env.CHROMIUM ? { executablePath: process.env.CHROMIUM } : {}),
    });
    browserVersion = browser.version();
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    evaluate = (action, input) => page.evaluate(action, input);
  }
  const { runs, validation } = await evaluate(async ({ fixtures, samples, warmup }) => {
    const open = async () => {
      const worker = new Worker("/probe.worker.js", { type: "module" });
      await new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error("worker initialization timed out")), 120_000);
        worker.onmessage = (event) => { if (event.data.ready) { clearTimeout(timeout); resolve(); } };
        worker.onerror = (event) => { clearTimeout(timeout); reject(new Error(event.message)); };
      });
      let id = 0;
      const request = (operation) => new Promise((resolve, reject) => {
        const at = performance.now();
        const expected = ++id;
        const timeout = setTimeout(() => reject(new Error(`${operation.action} timed out`)), 120_000);
        worker.onmessage = (event) => {
          clearTimeout(timeout);
          if (event.data.error) reject(new Error(event.data.error));
          else if (event.data.id !== expected) reject(new Error("stale worker reply"));
          else resolve({ ...event.data, roundTripMs: performance.now() - at });
        };
        worker.onerror = (event) => { clearTimeout(timeout); reject(new Error(event.message)); };
        worker.postMessage({ ...operation, id: expected });
      });
      return { worker, request };
    };
    const results = [];
    for (const fixture of fixtures) {
      for (const scenario of ["diagnostics-only", "edit-first-emit", "checked-first-emit", "unchanged-profile", "check-after-direct-run", "large-check-small-run"]) {
        const { worker, request } = await open();
        const loaded = await request({ action: "load" });
        const observations = [];
        let peak = loaded.measurement.linearMemoryBytes;
        for (let i = -warmup; i < samples; ++i) {
          const source = fixture.source + "\n// edit " + i + "\n";
          if (scenario === "checked-first-emit") {
            const checked = await request({ action: "check", source });
            if (checked.result.some((d) => d.severity === "error")) throw new Error(JSON.stringify(checked.result));
            peak = Math.max(peak, checked.measurement.linearMemoryBytes);
          } else if (scenario === "large-check-small-run") {
            const larger = source + Array.from({ length: 128 }, (_, n) => `\nvalue${n} : Int\nlet value${n} = ${n}\n`).join("");
            const checked = await request({ action: "check", source: larger });
            if (checked.result.some((d) => d.severity === "error")) throw new Error(JSON.stringify(checked.result));
            peak = Math.max(peak, checked.measurement.linearMemoryBytes);
          } else if (scenario === "unchanged-profile" || scenario === "check-after-direct-run") {
            const built = await request({ action: "build", source, profile: "debug" });
            if (!built.result.files || !built.result.entry) throw new Error(JSON.stringify(built.result));
            peak = Math.max(peak, built.measurement.linearMemoryBytes);
          }
          const checking = scenario === "diagnostics-only" || scenario === "check-after-direct-run";
          const operation = checking
            ? { action: "check", source: scenario === "check-after-direct-run" ? source + "// next check\n" : source }
            : { action: "build", source, profile: scenario === "unchanged-profile" ? "release" : "debug" };
          const measured = await request(operation);
          if (checking) {
            if (measured.result.some((d) => d.severity === "error")) throw new Error(JSON.stringify(measured.result));
          } else if (!measured.result.files || !measured.result.entry) {
            throw new Error(JSON.stringify(measured.result));
          }
          peak = Math.max(peak, measured.measurement.linearMemoryBytes);
          if (i >= 0) observations.push({ roundTripMs: measured.roundTripMs, ...measured.measurement });
        }
        await worker.terminate();
        results.push({ fixture: fixture.name, scenario, compilerVersion: loaded.result,
          initialLinearMemoryBytes: loaded.measurement.linearMemoryBytes, peakLinearMemoryBytes: peak, observations });
      }
    }
    // Exercise the production worker's revision semantics separately from
    // timing. Hash the complete payload, including JS, declarations, file
    // map, entry and diagnostics, for exact baseline/candidate comparison.
    const validation = [];
    const { worker, request } = await open();
    const original = 'module Main exposing (answer, main)\n// привет 😀\nanswer : String\nlet answer = "first"\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line answer\n';
    const revisions = [original, original.replace('"first"', '"second 🎉"'), original.replace('"first"', '42'), original.replace('"first"', '"fixed"'), "", original];
    for (let revision = 0; revision < revisions.length; ++revision) {
      const source = revisions[revision];
      for (const profile of ["debug", "release"]) {
        const { result } = await request({ action: "compile", source, profile, declarations: true });
        const checked = await request({ action: "check", source });
        if (JSON.stringify(result.diagnostics) !== JSON.stringify(checked.result)) throw new Error("compile/check diagnostics disagree");
        if (revision === 2) {
          if (result.code !== "" || result.dts !== "" || result.built.files !== null || result.built.entry !== null) throw new Error("stale output after invalid edit");
          const error = result.diagnostics.find((d) => d.severity === "error");
          const start = source.indexOf("42");
          if (!error || error.start !== start || error.end !== start + 2) throw new Error("invalid UTF-16 diagnostic range");
        } else if (source && (!result.code || !result.built.entry)) throw new Error("fixed source did not compile");
        if (source) {
          const at = source.lastIndexOf("answer");
          const hover = await request({ action: "hover", source, at });
          if (hover.result?.from !== at || hover.result?.to !== at + 6) throw new Error("stale hover range after compile");
        }
        const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify(result)));
        validation.push({ revision, profile, sha256: Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("") });
      }
    }
    await worker.terminate();
    return { runs: results, validation };
  }, { fixtures, samples, warmup });
  const distribution = (values) => {
    assert.ok(values.every((v) => Number.isFinite(v) && v >= 0), "invalid measured duration");
    const sorted = [...values].sort((a, b) => a - b);
    const percentile = (p) => sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * p) - 1)];
    return { min: sorted[0], p50: percentile(0.5), p95: percentile(0.95), max: sorted.at(-1) };
  };
  const report = {
    schema: 1, measuredAt: new Date().toISOString(),
    revision: process.env.BUILD_REVISION ?? execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
    harnessRevision: execFileSync("git", ["rev-parse", "HEAD"], { cwd: root, encoding: "utf8" }).trim(),
    workingTree: execFileSync("git", ["status", "--porcelain"], { cwd: root, encoding: "utf8" }).trim(),
    host: { platform: platform(), arch: arch(), cpu: cpus()[0]?.model, node: process.version, engine, chromium: browserVersion, nodeV8: process.versions.v8 },
    samples, warmup, workerSha256: hash(workerSource),
    wasm: { sha256: hash(wasmBytes), rawBytes: wasmBytes.length, gzipBytes: gzipSync(wasmBytes, { level: 6 }).length },
    fixtures: fixtures.map(({ name, sha256 }) => ({ name, sha256 })),
    validation,
    runs: runs.map((run) => ({ ...run,
      roundTripMs: distribution(run.observations.map((o) => o.roundTripMs)),
      workerMs: distribution(run.observations.map((o) => o.workerMs)),
      stages: Object.fromEntries([...new Set(run.observations.flatMap((o) => o.calls.map((c) => c.name)))].map((name) => [
        name, distribution(run.observations.map((o) => o.calls.filter((c) => c.name === name).reduce((sum, c) => sum + c.ms, 0))),
      ])),
      instrumentedStages: Object.fromEntries([...new Set(run.observations.flatMap((o) => o.stages.map((s) => s.id)))].map((id) => [
        ["", "parse", "resolve-check", "lower", "backend", "output-map", "json-stringify"][id],
        { exclusiveMs: distribution(run.observations.map((o) => o.stages.filter((s) => s.id === id).reduce((sum, s) => sum + s.exclusiveMs, 0))),
          inclusiveMs: distribution(run.observations.map((o) => o.stages.filter((s) => s.id === id).reduce((sum, s) => sum + s.inclusiveMs, 0))) },
      ])),
    })),
  };
  const json = JSON.stringify(report, null, 2) + "\n";
  if (output) writeFileSync(output, json);
  else process.stdout.write(json);
  for (const run of report.runs) console.error(`${run.fixture} ${run.scenario}: ${run.workerMs.p50.toFixed(2)} ms p50, ${run.workerMs.p95.toFixed(2)} ms p95; ${(run.peakLinearMemoryBytes / 1048576).toFixed(2)} MiB peak linear memory`);
} finally {
  await browser?.close();
  if (server.listening) await new Promise((resolve) => server.close(resolve));
}
