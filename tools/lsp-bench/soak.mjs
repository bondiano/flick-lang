// Holds the language server to a long session and prints its resident
// memory: the same file opened, edited and closed again and again, then
// as many distinct standalone copies of it, each opened, edited and closed
// once. Memory that keeps growing in the second phase is state a closed
// document leaves behind: the last line is how much it grew over that
// phase. Linux only: it reads /proc.
//
//   node tools/lsp-bench/soak.mjs <flick binary> <file.fl> [cycles]
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

import { connect } from "./client.mjs";

const [binary, file, cycles = "400"] = process.argv.slice(2);
if (!binary || !file) {
  console.error("usage: soak.mjs <flick binary> <file.fl> [cycles]");
  process.exit(2);
}
const total = Number(cycles);
const text = readFileSync(file, "utf8");
// outside any project, so each copy is a standalone context of its own
const dir = mkdtempSync(join(tmpdir(), "flick-lsp-soak-"));
const { server, send, request } = connect(binary);
const rss = () => Number(/VmRSS:\s+(\d+)/.exec(readFileSync(`/proc/${server.pid}/status`, "utf8"))[1]) / 1024;
const report = (label) => console.log(`${label.padEnd(28)} ${rss().toFixed(1).padStart(8)} MiB`);

// a request answers only after the notifications before it are handled
const settled = () => request("workspace/symbol", { query: "" });
const cycle = async (path) => {
  const uri = pathToFileURL(path).href;
  send({ method: "textDocument/didOpen", params: { textDocument: { uri, languageId: "flick", version: 1, text } } });
  await request("textDocument/hover", { textDocument: { uri }, position: { line: 0, character: 0 } });
  send({ method: "textDocument/didChange", params: { textDocument: { uri, version: 2 }, contentChanges: [{ text: `${text}\n` }] } });
  await request("textDocument/hover", { textDocument: { uri }, position: { line: 0, character: 0 } });
  send({ method: "textDocument/didClose", params: { textDocument: { uri } } });
  await settled();
};
const phase = async (label, pathOf) => {
  const before = rss();
  for (let i = 1; i <= total; i++) {
    await cycle(pathOf(i));
    if (i % (total / 4) === 0) report(`${label} ${i}/${total}`);
  }
  return rss() - before;
};

await request("initialize", { capabilities: {} });
send({ method: "initialized", params: {} });
report("initialized");
const same = join(dir, "Same.fl");
writeFileSync(same, text);
await phase("same file", () => same);
const growth = await phase("distinct files", (i) => {
  const path = join(dir, `Copy${i}.fl`);
  writeFileSync(path, text);
  return path;
});
console.log(`RSS, MiB: growth ${Math.max(0, growth).toFixed(1)}`);
await request("shutdown", null);
send({ method: "exit" });
rmSync(dir, { recursive: true, force: true });
