// Measure the real proof editor requests on one project and soak repeated
// proof edits in the same and distinct workspaces.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { cpSync, createReadStream, mkdirSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { cpus, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

import { readRss, sampleRss } from "../perf/rss.mjs";
import { connect } from "./client.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const FIXTURE = join(HERE, "fixtures/proofs");
const OPTION_LAWS = join(HERE, "../../packages/core/proofs/Laws/Option.fl");

function edit(text, before, after) {
  const offset = text.indexOf(before);
  assert.ok(offset >= 0 && text.lastIndexOf(before) === offset, `unique fixture marker: ${before}`);
  return text.slice(0, offset) + after + text.slice(offset + before.length);
}

function position(text, marker, inside = 0) {
  const offset = text.indexOf(marker);
  assert.ok(offset >= 0, `fixture marker: ${marker}`);
  const lines = text.slice(0, offset + inside).split("\n");
  return { line: lines.length - 1, character: lines.at(-1).length };
}

function project(root) {
  cpSync(FIXTURE, root, { recursive: true });
  const laws = join(root, "proofs/Laws");
  mkdirSync(laws);
  cpSync(OPTION_LAWS, join(laws, "Option.fl"));
  const document = (file) => ({
    uri: pathToFileURL(join(root, file)).href,
    text: readFileSync(join(root, file), "utf8"),
  });
  return {
    uri: pathToFileURL(root).href,
    main: document("proofs/Bench/Main.fl"),
    dependency: document("src/Bench/Steps.fl"),
    optionLaws: document("proofs/Laws/Option.fl"),
  };
}

function changeSignal() {
  let value = 0;
  let wake;
  return {
    notice(message) {
      if (message.method !== "flick/proofStateChanged") return;
      value++;
      wake?.();
      wake = undefined;
    },
    current: () => value,
    after(before) {
      return before === value ? new Promise((done) => { wake = done; }) : Promise.resolve();
    },
  };
}

function workload(client, signal) {
  const { send, request } = client;
  const ask = async (method, params) => {
    const reply = await request(method, params);
    assert.equal(reply.error, undefined, `${method}: ${JSON.stringify(reply.error)}`);
    return reply.result;
  };
  const opened = (doc, version = 1) => send({
    method: "textDocument/didOpen",
    params: { textDocument: { ...doc, languageId: "flick", version } },
  });
  const changed = (doc, version, text) => send({
    method: "textDocument/didChange",
    params: { textDocument: { uri: doc.uri, version }, contentChanges: [{ text }] },
  });
  const closed = (doc) => send({
    method: "textDocument/didClose",
    params: { textDocument: { uri: doc.uri } },
  });
  const query = (method, doc, at) => ask(method, { textDocument: { uri: doc.uri }, position: at });
  const inspect = (doc, at) => query("flick/proofAt", doc, at);
  const ready = async (doc, at, { version, name, status }) => {
    for (;;) {
      const before = signal.current();
      const current = await inspect(doc, at);
      assert.equal(current.version, version);
      assert.deepEqual(current.position, at);
      if (!current.pending) {
        assert.equal(current.proof?.name, name, JSON.stringify(current));
        assert.equal(current.proof.status, status, JSON.stringify(current));
        return current;
      }
      assert.equal(current.proof, null, "a pending revision must not carry an old checked proof");
      await signal.after(before);
    }
  };
  return { ask, opened, changed, closed, query, inspect, ready };
}

function quantiles(values) {
  const ordered = [...values].sort((a, b) => a - b);
  const at = (fraction) => ordered[Math.ceil(fraction * ordered.length) - 1];
  return { count: values.length, p50: at(0.5), p95: at(0.95) };
}

async function digest(file) {
  const hash = createHash("sha256");
  for await (const bytes of createReadStream(file)) hash.update(bytes);
  return hash.digest("hex");
}

async function timed(work) {
  const started = performance.now();
  const value = await work();
  return { value, milliseconds: performance.now() - started };
}

const CASE_BODY = "\n  match v with\n  | None -> refl\n  | Some value -> refl\n";
const CHAIN_LEMMA = "Chain.twice_shared";
const casePosition = (text) => position(text, CASE_BODY, 3);
const wrongCase = (text) => edit(text, CASE_BODY, "\n  refl\n");
const state = (version, name, status = "checked") => ({ version, name, status });

async function latency(run, { main, dependency, optionLaws }, opts) {
  const atCase = casePosition(main.text);
  const atLemma = position(main.text, CHAIN_LEMMA, "Chain.".length + 1);
  const readyCase = (version, status) => run.ready(main, atCase, state(version, "rebuild", status));
  let mainVersion = 1;
  const opened = await timed(async () => {
    run.opened(main);
    return readyCase(mainVersion);
  });
  const initial = opened.value;
  assert.ok(initial.proof.normalizationSteps > 0, "case split must be checked by the verifier");
  run.opened(optionLaws);
  const bundle = await run.ready(optionLaws, position(optionLaws.text, "lawful Monad Option"), state(1, "option"));
  assert.ok(bundle.proof.obligations.some(({ law, path, status }) =>
    law === "identity" && path.includes("Functor") && status === "checked"),
  "Option workload must verify an inherited Functor law");
  run.closed(optionLaws);
  await readyCase(mainVersion);

  const measurements = {};
  const sample = async (key, work) => {
    const values = [];
    for (let i = 0; i < opts.warmup + opts.samples; i++) {
      const { milliseconds } = await timed(() => work(i));
      if (i >= opts.warmup) values.push(milliseconds);
    }
    measurements[key] = quantiles(values);
  };

  await sample("warmProofStatus", () => readyCase(mainVersion));
  if (!opts.statusOnly) {
    await sample("warmProofHover", async () => {
      const hover = await run.query("textDocument/hover", main, atLemma);
      assert.ok(hover?.contents, "a checked lemma has proof hover");
    });
    const draft = edit(main.text, "= symm h\n", "= \n");
    const atDraft = position(draft, "= \n", 2);
    run.changed(main, ++mainVersion, draft);
    await run.ready(main, atDraft, state(mainVersion, "backwards", "open"));
    await sample("warmGoalCompletion", async () => {
      const result = await run.query("textDocument/completion", main, atDraft);
      const items = Array.isArray(result) ? result : result?.items;
      assert.ok(items?.some((item) => [item.label, item.insertText, item.textEdit?.newText].includes("symm h")),
        "completion must offer a proof of the live goal");
    });
    run.changed(main, ++mainVersion, main.text);
    await readyCase(mainVersion);
  }

  const incorrect = wrongCase(main.text);
  await sample("proofStatusAfterBodyEdit", async (i) => {
    const broken = i % 2 === 0;
    run.changed(main, ++mainVersion, broken ? incorrect : main.text);
    await readyCase(mainVersion, broken ? "open" : "checked");
  });
  run.changed(main, ++mainVersion, main.text);
  const restored = await readyCase(mainVersion);
  assert.ok(restored.generation > initial.generation);

  run.opened(dependency);
  let dependencyVersion = 1;
  const readyLemma = (status) => run.ready(main, atLemma, state(mainVersion, "use_twice", status));
  await readyLemma("checked");
  const wrongDependency = edit(dependency.text, "f (f x)", "f x");
  await sample("proofStatusAfterDependencyBodyEdit", async (i) => {
    const broken = i % 2 === 0;
    run.changed(dependency, ++dependencyVersion, broken ? wrongDependency : dependency.text);
    const current = await readyLemma(broken ? "open" : "checked");
    assert.equal(current.proof.lemmas.includes(CHAIN_LEMMA), !broken, "checked lemma must follow the dependency");
  });

  // The first edit must never produce a checked result for the superseding
  // text. Restoring the dependency makes the same main proof checked again.
  run.changed(dependency, ++dependencyVersion, dependency.text);
  const beforeBurst = await readyLemma("checked");
  run.changed(dependency, ++dependencyVersion, `${dependency.text}\n`);
  run.changed(dependency, ++dependencyVersion, wrongDependency);
  const superseding = await readyLemma("open");
  assert.ok(superseding.generation > beforeBurst.generation);
  assert.ok(!superseding.proof.lemmas.includes(CHAIN_LEMMA));
  run.changed(dependency, ++dependencyVersion, dependency.text);
  assert.ok((await readyLemma("checked")).proof.lemmas.includes(CHAIN_LEMMA));

  run.closed(dependency);
  run.closed(main);
  const afterClose = await run.inspect(main, atCase);
  assert.equal(afterClose.version, null);
  assert.equal(afterClose.proof, null);
  return {
    firstProofOpenMs: opened.milliseconds,
    normalizationSteps: { rebuild: initial.proof.normalizationSteps },
    timingsMs: measurements,
  };
}

async function soak(run, workspace, directory, cycles, rss) {
  const cycle = async ({ main }) => {
    const at = casePosition(main.text);
    run.opened(main);
    await run.ready(main, at, state(1, "rebuild"));
    run.changed(main, 2, wrongCase(main.text));
    await run.ready(main, at, state(2, "rebuild", "open"));
    run.closed(main);
    assert.equal((await run.inspect(main, at)).proof, null);
  };
  const phase = async (make) => {
    const before = rss();
    const checkpoints = [];
    const stride = Math.ceil(cycles / 4);
    for (let i = 1; i <= cycles; i++) {
      await cycle(make(i));
      if (i % stride === 0 || i === cycles) checkpoints.push({ cycle: i, rssMiB: rss() });
    }
    const after = checkpoints.at(-1).rssMiB;
    return { before, after, growth: after - before, checkpoints };
  };
  return {
    cycles,
    sameWorkspace: await phase(() => workspace),
    distinctWorkspaces: await phase((i) => project(join(directory, `workspace-${i}`))),
  };
}

async function measure(binary, fixture, directory, opts) {
  const signal = changeSignal();
  const started = performance.now();
  const client = connect(binary, { onNotification: signal.notice });
  const { server } = client;
  let watchdog;
  let rss;
  let shuttingDown = false;
  try {
    const exited = new Promise((resolve) => server.once("exit", (code, signal) => resolve({ code, signal })));
    const stopped = new Promise((_, reject) => {
      server.once("error", reject);
      server.once("exit", (code, signal) => {
        if (!shuttingDown) reject(new Error(`Flick LSP exited: ${code ?? signal}`));
      });
      watchdog = setTimeout(() => reject(new Error("proof LSP workload timed out")), 300_000);
    });
    const run = workload(client, signal);
    const session = async () => {
      await run.ask("initialize", { capabilities: {}, rootUri: fixture.uri });
      const startupMs = performance.now() - started;
      client.send({ method: "initialized", params: {} });
      rss = sampleRss(server.pid);
      const timings = await latency(run, fixture, opts);
      const retainedRssMiB = opts.cycles === 0
        ? null
        : await soak(run, fixture, directory, opts.cycles, () => readRss(server.pid).current);
      return { startupMs, ...timings, retainedRssMiB };
    };
    const result = await Promise.race([session(), stopped]);
    const peakRssMiB = rss.stop();
    rss = undefined;
    await Promise.race([run.ask("shutdown", null), stopped]);
    shuttingDown = true;
    client.send({ method: "exit" });
    const exit = await Promise.race([exited, stopped]);
    assert.equal(exit.code, 0, `Flick LSP shutdown: ${exit.signal ?? exit.code}`);
    return { ...result, peakRssMiB };
  } finally {
    clearTimeout(watchdog);
    if (server.exitCode === null && server.signalCode === null) {
      try {
        rss?.stop();
      } finally {
        server.kill();
      }
    }
  }
}

const { values, positionals } = parseArgs({
  options: {
    samples: { type: "string", default: "30" },
    warmup: { type: "string", default: "5" },
    cycles: { type: "string", default: "100" },
    "status-only": { type: "boolean", default: false },
  },
  allowPositionals: true,
});
assert.equal(positionals.length, 1,
  "usage: proofs.mjs [--samples N] [--warmup N] [--cycles N] [--status-only] <flick binary>");
const opts = {
  samples: Number(values.samples),
  warmup: Number(values.warmup),
  cycles: Number(values.cycles),
  statusOnly: values["status-only"],
};
assert.ok(Number.isSafeInteger(opts.samples) && opts.samples > 0);
assert.ok([opts.warmup, opts.cycles].every((n) => Number.isSafeInteger(n) && n >= 0));
const binary = resolve(positionals[0]);
const binarySha256 = await digest(binary);
const directory = mkdtempSync(join(tmpdir(), "flick-proof-lsp-"));
try {
  const result = await measure(binary, project(join(directory, "baseline")), directory, opts);
  console.log(JSON.stringify({
    platform: process.platform,
    arch: process.arch,
    cpu: cpus()[0]?.model,
    node: process.version,
    binary,
    binarySha256,
    ...opts,
    ...result,
  }, null, 2));
} finally {
  rmSync(directory, { recursive: true, force: true });
}
