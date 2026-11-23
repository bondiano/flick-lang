// Matched release LSP sessions: a small and a generated wide import graph.
// Usage: node tools/perf/module-cache-edits.mjs <before-binary> <after-binary>
// Optional: --samples 30 --rounds 3 --warmup 5 --widths 10,1000
// Run while other builds/benchmarks are idle. Each round starts a fresh server
// for each variant, alternating their order; all sessions replay the same edits.
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { cpus, tmpdir } from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { connect } from "../lsp-bench/client.mjs";
import { sampleRss } from "./rss.mjs";

const { values, positionals } = parseArgs({
  options: {
    samples: { type: "string", default: "30" },
    rounds: { type: "string", default: "3" },
    warmup: { type: "string", default: "5" },
    widths: { type: "string", default: "10,1000" },
  },
  allowPositionals: true,
});
assert.equal(positionals.length, 2, "provide before and after release binaries");
const binaries = positionals.map((binary) => path.resolve(binary));
const samples = Number(values.samples);
const rounds = Number(values.rounds);
const warmup = Number(values.warmup);
const widths = values.widths.split(",").map(Number);
assert.ok([samples, rounds, ...widths].every((n) => Number.isSafeInteger(n) && n > 0));
assert.ok(Number.isSafeInteger(warmup) && warmup >= 0);
const directory = mkdtempSync(path.join(tmpdir(), "flick-prune-edits-"));
const summary = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const at = (p) => sorted[Math.ceil(sorted.length * p) - 1];
  return { samples: sorted.length, min: sorted[0], p50: at(0.5), p95: at(0.95), max: sorted.at(-1) };
};
console.log(JSON.stringify({ cpu: cpus()[0]?.model, platform: process.platform, arch: process.arch, node: process.version, binaries, warmup, samples, rounds, widths }));
try {
  for (const width of widths) {
    const project = path.join(directory, String(width));
    const src = path.join(project, "src", "Bench");
    mkdirSync(src, { recursive: true });
    writeFileSync(path.join(project, "flick.toml"), 'schema-version = 1\n[project]\nkind = "lib"\nsource-dir = "src"\n');
    const imports = [];
    for (let i = 0; i < width; i++) {
      const name = `Leaf${String(i).padStart(4, "0")}`;
      imports.push(`import Bench.${name}`);
      writeFileSync(path.join(src, `${name}.fl`), `module Bench.${name} exposing (value)\nvalue : Int\nlet value = ${i}\n`);
    }
    const text = `module Bench.Main exposing (answer)\n${imports.join("\n")}\nanswer : Int\nlet answer = 1\n`;
    const file = path.join(src, "Main.fl");
    const uri = pathToFileURL(file).href;
    writeFileSync(file, text);
    const results = [[], []];
    const peaks = [[], []];
    let expected;
    for (let round = 0; round < rounds; round++) {
      for (const variant of round % 2 === 0 ? [0, 1] : [1, 0]) {
        const client = connect(binaries[variant]);
        const memory = sampleRss(client.server.pid);
        const timeout = setTimeout(() => client.server.kill(), 120_000);
        const exited = new Promise((_, reject) => {
          client.server.once("error", reject);
          client.server.once("exit", (code, signal) => reject(new Error(`LSP exited: ${code ?? signal}`)));
        });
        const session = async () => {
          await client.request("initialize", { capabilities: {}, rootUri: pathToFileURL(project).href });
          client.send({ method: "initialized", params: {} });
          const opened = client.diagnostics(uri);
          client.send({ method: "textDocument/didOpen", params: { textDocument: { uri, languageId: "flick", version: 1, text } } });
          const diagnostic = await opened;
          assert.ok(diagnostic.params.diagnostics.every((d) => d.severity !== 1), JSON.stringify(diagnostic.params.diagnostics));
          const hover = () => client.request("textDocument/hover", { textDocument: { uri }, position: { line: width + 2, character: 6 } });
          const first = await hover();
          assert.ok(first.result, "fixture has a hover target");
          const observation = { diagnostics: diagnostic.params.diagnostics, hover: first.result };
          expected ??= observation;
          assert.deepEqual(observation, expected, "identical initial session state and outputs");
          for (let i = 0; i < warmup + samples; i++) {
            const started = performance.now();
            client.send({ method: "textDocument/didChange", params: { textDocument: { uri, version: i + 2 }, contentChanges: [{ text: `${text}// edit ${i % 2}\n` }] } });
            const reply = await hover();
            const elapsed = performance.now() - started;
            assert.deepEqual(reply.result, first.result, "the warm edit preserves entities and locations");
            if (i >= warmup) results[variant].push(elapsed);
          }
        };
        try {
          await Promise.race([session(), exited]);
        } finally {
          clearTimeout(timeout);
          peaks[variant].push(memory.stop());
          client.server.kill();
          await new Promise((resolve) => {
            if (client.server.exitCode !== null || client.server.signalCode !== null) resolve();
            else client.server.once("exit", resolve);
          });
        }
        console.log(JSON.stringify({ width, round, variant, milliseconds: summary(results[variant]), peakRssMiB: Math.max(...peaks[variant]) }));
      }
    }
  }
} finally {
  rmSync(directory, { recursive: true, force: true });
}
