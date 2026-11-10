// Times the language server on one file: the first and a repeated open,
// a hover, and a hover right after an edit in the middle of the file,
// which moves every declaration below it.
//
// With `--repeat N` it then takes N more samples of each hover and of a
// hover after an edit, and prints their p50 and p95.
//
//   node tools/lsp-bench/index.mjs [--repeat N] <flick binary> <file.fl> <line:character>...
//
// Lines and characters are zero-based, as in LSP.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

import { connect } from "./client.mjs";
import { sampleRss } from "../perf/rss.mjs";

const {
  values: { repeat },
  positionals: [binary, file, ...positions],
} = parseArgs({ options: { repeat: { type: "string", default: "0" } }, allowPositionals: true });
if (!binary || !file || positions.length === 0) {
  console.error("usage: index.mjs [--repeat N] <flick binary> <file.fl> <line:character>...");
  process.exit(2);
}
const uri = pathToFileURL(resolve(file)).href;
const text = readFileSync(file, "utf8");
const { server, send, request, diagnostics: published } = connect(binary);
const memory = sampleRss(server.pid);
const diagnostics = () => published(uri);
const timed = async (label, work) => {
  const started = performance.now();
  const result = await work();
  console.log(`${label.padEnd(24)} ${(performance.now() - started).toFixed(0).padStart(5)} ms`);
  return result;
};
const open = () => {
  const published = diagnostics();
  send({
    method: "textDocument/didOpen",
    params: { textDocument: { uri, languageId: "flick", version: 1, text } },
  });
  return published;
};
const hover = ([line, character]) =>
  request("textDocument/hover", { textDocument: { uri }, position: { line, character } });

await timed("initialize", () => request("initialize", { capabilities: {} }));
send({ method: "initialized", params: {} });
await timed("first open", open);
const closed = diagnostics();
send({ method: "textDocument/didClose", params: { textDocument: { uri } } });
await closed;
await timed("repeated open", open);
for (const position of positions) {
  const at = position.split(":").map(Number);
  const reply = await timed(`hover ${position}`, () => hover(at));
  if (!reply.result) console.log("  (no hover)");
}
const lines = text.split("\n");
const middle = Math.floor(lines.length / 2);
const edited = [...lines.slice(0, middle), "", ...lines.slice(middle)].join("\n");
send({
  method: "textDocument/didChange",
  params: { textDocument: { uri, version: 2 }, contentChanges: [{ text: edited }] },
});
const [line, character] = positions[0].split(":").map(Number);
await timed("hover after edit", () => hover([line < middle ? line : line + 1, character]));

if (Number(repeat) > 0) {
  const sample = async (work) => {
    const started = performance.now();
    await work();
    return performance.now() - started;
  };
  const samples = { hover: [], "hover after edit": [] };
  for (let i = 0; i < Number(repeat); i++) {
    // the edit takes the blank line out and puts it back in turn: each moves every declaration below it
    const blank = i % 2 === 1;
    send({
      method: "textDocument/didChange",
      params: { textDocument: { uri, version: 3 + i }, contentChanges: [{ text: blank ? edited : text }] },
    });
    const shifted = (position) => {
      const [l, c] = position.split(":").map(Number);
      return [blank && l >= middle ? l + 1 : l, c];
    };
    samples["hover after edit"].push(await sample(() => hover(shifted(positions[0]))));
    for (const position of positions) samples.hover.push(await sample(() => hover(shifted(position))));
  }
  for (const [label, ms] of Object.entries(samples)) {
    ms.sort((a, b) => a - b);
    // nearest rank: the smallest sample with at least `percent` of them at or below it
    const at = (percent) => ms[Math.ceil((percent * ms.length) / 100) - 1].toFixed(1);
    console.log(`${`${label}, ${ms.length} samples`.padEnd(32)} p50 ${at(50).padStart(6)} ms  p95 ${at(95).padStart(6)} ms`);
  }
}
console.log(`RSS, MiB: peak ${memory.stop().toFixed(1)}`);
server.kill();
