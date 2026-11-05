// Output tokens of a fullstack app: the Conduit example, the contract, the
// Go server and the browser client, against the RealWorld reference of the
// same app, a Go (gin) backend and a React/Redux client, each pinned to a
// commit. Prints the tokens of each part with o200k_base, and with Claude's
// tokenizer through Anthropic's token counting endpoint when
// ANTHROPIC_API_KEY is set, or else through OpenRouter when
// OPENROUTER_API_KEY is set. The reference sources are downloaded once into
// node_modules/.cache/flick-tokens and only read.
//
//   node tools/tokens/index.mjs
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { countTokens } from "gpt-tokenizer/encoding/o200k_base";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** The reference, read from its tarball at these commits, never run or imported. */
const REFERENCE = {
  server: { repo: "gothinkster/golang-gin-realworld-example-app", sha: "a41289b2a4951e7cc09066f86742a27f85121c9b" },
  client: { repo: "gothinkster/react-redux-realworld-example-app", sha: "ee72eba4056392c95a27bc48d385d3f54ba38a18" },
};

/**
 * What is counted: hand-written source only. A file counts for its part when
 * its path, relative to the side's root, matches `include` and no pattern of
 * EXCLUDE, and it is not generated. The contract counts with the server: it
 * is the API, as the Go server's models, serializers and validators are.
 */
const SCOPE = [
  { side: "flick", part: "server", include: /^(contract|server)\/src\/.+\.fl$/ },
  { side: "flick", part: "client", include: /^client\/src\/.+\.(fl|js)$/ },
  { side: "reference", part: "server", include: /\.go$/ },
  { side: "reference", part: "client", include: /^src\/.+\.jsx?$/ },
];

/** Never counted: tests and their helpers, vendored, built and generated files. */
const EXCLUDE = [
  /_test\.go$/,
  /(^|\/)test_helpers\.go$/,
  /\.(test|spec)\.[jt]sx?$/,
  /(^|\/)(test|tests|__tests__|vendor|node_modules|dist|\.flick)\//,
  /\.d\.fl\.ts$/,
];

/** The Go convention for the header of a generated file. */
const GENERATED = /^\/\/ Code generated .* DO NOT EDIT\.$/m;

const CLAUDE_MODEL = "claude-opus-5-5";
const OPENROUTER_MODEL = "anthropic/claude-opus-5.5";

/** The sources of `repo` at `sha`, downloaded and extracted once. */
const download = async ({ repo, sha }) => {
  const dir = path.join(root, "node_modules/.cache/flick-tokens", `${repo}@${sha}`);
  if (existsSync(dir)) return dir;
  const response = await fetch(`https://codeload.github.com/${repo}/tar.gz/${sha}`);
  if (!response.ok) throw new Error(`downloading ${repo}@${sha}: HTTP ${response.status}`);
  // extracted beside the cache entry first, so an interrupted run leaves no partial entry
  const partial = `${dir}.partial`;
  rmSync(partial, { recursive: true, force: true });
  mkdirSync(partial, { recursive: true });
  execFileSync("tar", ["-xz", "-C", partial, "--strip-components=1"], {
    input: Buffer.from(await response.arrayBuffer()),
  });
  renameSync(partial, dir);
  return dir;
};

/** The files of a part under `dir`, sorted, with their text; symbolic links are not followed. */
const files = (dir, include) =>
  readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => path.relative(dir, path.join(entry.parentPath, entry.name)).split(path.sep).join("/"))
    .filter((file) => include.test(file) && !EXCLUDE.some((pattern) => pattern.test(file)))
    .sort()
    .map((file) => ({ file, text: readFileSync(path.join(dir, file), "utf8") }))
    .filter(({ text }) => !GENERATED.test(text));

/** Claude's count of `text` as one user message, free. */
const anthropic = async (text) => {
  const response = await fetch("https://api.anthropic.com/v1/messages/count_tokens", {
    method: "POST",
    headers: {
      "x-api-key": process.env.ANTHROPIC_API_KEY,
      "anthropic-version": "2023-06-01",
      "content-type": "application/json",
    },
    body: JSON.stringify({ model: CLAUDE_MODEL, messages: [{ role: "user", content: text }] }),
  });
  if (!response.ok) throw new Error(`count_tokens: HTTP ${response.status} ${await response.text()}`);
  return (await response.json()).input_tokens;
};

/** The same count as the prompt of a one-token completion, which OpenRouter
 * reports in the model's own tokenizer; the prompt is billed. */
const openRouter = async (text) => {
  const response = await fetch("https://openrouter.ai/api/v1/chat/completions", {
    method: "POST",
    headers: { authorization: `Bearer ${process.env.OPENROUTER_API_KEY}`, "content-type": "application/json" },
    body: JSON.stringify({ model: OPENROUTER_MODEL, max_tokens: 1, messages: [{ role: "user", content: text }] }),
  });
  if (!response.ok) throw new Error(`OpenRouter: HTTP ${response.status} ${await response.text()}`);
  return (await response.json()).usage.prompt_tokens;
};

const table = (title, counts) => {
  const total = (side) => counts.server[side] + counts.client[side];
  const rows = [...["server", "client"].map((part) => [part, counts[part]]), ["total", { flick: total("flick"), reference: total("reference") }]];
  console.log(`\n${title}\n\n| part | Flick | reference | ratio |\n|---|---:|---:|---:|`);
  for (const [part, { flick, reference }] of rows) {
    console.log(`| ${part} | ${flick} | ${reference} | ${(flick / reference).toFixed(2)} |`);
  }
};

const main = async () => {
  const roots = {
    flick: { server: path.join(root, "examples/conduit"), client: path.join(root, "examples/conduit") },
    reference: { server: await download(REFERENCE.server), client: await download(REFERENCE.client) },
  };
  // the files of a part, joined by a blank line: one text per side and part
  const parts = SCOPE.map(({ side, part, include }) => {
    const found = files(roots[side][part], include);
    if (found.length === 0) throw new Error(`no ${side} ${part} files match ${include}`);
    const text = found.map((f) => f.text).join("\n\n");
    return { side, part, files: found.length, bytes: Buffer.byteLength(text), text };
  });
  const by = (key) => {
    const counts = { server: {}, client: {} };
    for (const p of parts) counts[p.part][p.side] = key(p);
    return counts;
  };

  console.log(`reference: ${REFERENCE.server.repo}@${REFERENCE.server.sha.slice(0, 12)}, ${REFERENCE.client.repo}@${REFERENCE.client.sha.slice(0, 12)}`);
  console.log("\n| part | Flick files | Flick bytes | reference files | reference bytes |\n|---|---:|---:|---:|---:|");
  for (const part of ["server", "client"]) {
    const [flick, reference] = ["flick", "reference"].map((side) => parts.find((p) => p.side === side && p.part === part));
    console.log(`| ${part} | ${flick.files} | ${flick.bytes} | ${reference.files} | ${reference.bytes} |`);
  }

  table("o200k_base (gpt-tokenizer)", by((p) => countTokens(p.text)));

  const counter = process.env.ANTHROPIC_API_KEY
    ? { count: anthropic, via: "count_tokens" }
    : process.env.OPENROUTER_API_KEY
      ? { count: openRouter, via: `OpenRouter ${OPENROUTER_MODEL}` }
      : null;
  if (!counter) {
    console.log(`\n${CLAUDE_MODEL}: skipped, neither ANTHROPIC_API_KEY nor OPENROUTER_API_KEY is set`);
    return;
  }
  // a message of one character counts as one token plus the message's own overhead
  const overhead = (await counter.count("x")) - 1;
  for (const p of parts) p.claude = (await counter.count(p.text)) - overhead;
  table(`${CLAUDE_MODEL} (${counter.via}, less ${overhead} tokens of message overhead)`, by((p) => p.claude));
};

await main();
