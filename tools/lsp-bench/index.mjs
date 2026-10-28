// Times the language server on one file: the first and a repeated open,
// a hover, and a hover right after an edit in the middle of the file,
// which moves every declaration below it.
//
//   node tools/lsp-bench/index.mjs <flick binary> <file.fl> <line:character>...
//
// Lines and characters are zero-based, as in LSP.
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

import { connect } from "./client.mjs";

const [binary, file, ...positions] = process.argv.slice(2);
if (!binary || !file || positions.length === 0) {
  console.error("usage: index.mjs <flick binary> <file.fl> <line:character>...");
  process.exit(2);
}
const uri = pathToFileURL(resolve(file)).href;
const text = readFileSync(file, "utf8");
const { server, send, request, diagnostics: published } = connect(binary);
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
server.kill();
