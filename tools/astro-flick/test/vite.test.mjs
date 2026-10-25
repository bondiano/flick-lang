import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { fileURLToPath, pathToFileURL } from "node:url";
import { promisify } from "node:util";
import { JSDOM } from "jsdom";

import flick from "../index.mjs";
import { flickVite } from "../vite.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../../..");
const binary = path.join(repo, "target/debug/flick");
const run = promisify(execFile);

const fixture = (t, name) => {
  const root = path.join(repo, "target/tmp");
  mkdirSync(root, { recursive: true });
  const dir = mkdtempSync(path.join(root, `astro-flick-${name}-`));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  return dir;
};

const load = async (plugin, id) => {
  const watched = [];
  const context = {
    addWatchFile: (f) => watched.push(f),
    error: (message) => {
      throw new Error(message);
    },
  };
  return { code: await plugin.load.call(context, id), watched };
};

const targetOf = (code) => JSON.parse(/^export \* from (.*);\n$/.exec(code)[1]);
const imported = (code) => import(pathToFileURL(targetOf(code)));
const observed = async (code) => {
  const { stdout } = await run(process.execPath, ["--input-type=module", "--eval", `
    const built = await import(${JSON.stringify(pathToFileURL(targetOf(code)).href)});
    console.log(JSON.stringify({ answer: built.answer, label: built.state.label?.[0] ?? null }));
  `]);
  return JSON.parse(stdout);
};

const project = (dir) => {
  mkdirSync(path.join(dir, "src/Ui"), { recursive: true });
  const file = path.join(dir, "src/Ui/Widget.fl");
  const shared = path.join(dir, "src/Shared.fl");
  const config = path.join(dir, "flick.toml");
  const settings = (labels, sourceDir = "src") => `schema-version = 1\n[project]\nkind = "lib"\nsource-dir = "${sourceDir}"\n[build]\nemit = "ts"\nlabels = "${labels}"\n`;
  const answer = (n) => `module Shared exposing (answer)\n\nanswer : Number\nlet answer = ${n}\n`;
  writeFileSync(config, settings("none"));
  writeFileSync(shared, answer(1));
  writeFileSync(file, `module Ui.Widget exposing (answer, state)

import Shared
import Reactivity.Atom as Atom

answer : Number
let answer = Shared.answer

state : Atom.Writable Number
let state = Atom.state answer
`);
  return { file, shared, config, settings, answer };
};

test("compilation keeps Vite responsive and rebuilds edited sources", async (t) => {
  const dir = fixture(t, "edits");
  const file = path.join(dir, "Greet.fl");
  writeFileSync(file, '// c\nmodule Greet exposing (greeting)\n\ngreeting : String\nlet greeting = "hi"\n');
  const plugin = flickVite({ binary, outDir: path.join(dir, "out") });
  let ticked = false;
  setImmediate(() => { ticked = true; });
  const first = await load(plugin, `${file}?import`);
  assert.equal(ticked, true, "compiling must not block Vite's event loop");
  assert.equal((await imported(first.code)).greeting, "hi");
  assert.ok(first.watched.includes(file));
  assert.ok(first.watched.includes(dir), "the compiler's standalone import root is watched");
  writeFileSync(file, 'module Greet exposing (greeting)\n\ngreeting : String\nlet greeting = "hello"\n');
  const second = await load(plugin, file);
  assert.notEqual(second.code, first.code, "an edit builds anew");
  assert.equal((await imported(second.code)).greeting, "hello");
  assert.equal((await load(plugin, path.join(dir, "x.js"))).code, null);
  writeFileSync(file, "module Greet exposing (greeting)\n\ngreeting : String\nlet greeting = 1\n");
  await assert.rejects(load(plugin, file), /FLICK-T003/);
});

test("JS output follows project-wide source and configuration changes", async (t) => {
  const dir = fixture(t, "project");
  const { file, shared, config, settings, answer } = project(dir);
  const plugin = flickVite({ binary, profile: "release", outDir: path.join(dir, "out") });
  const first = await load(plugin, file);
  assert.match(targetOf(first.code), /Widget\.js$/);
  assert.deepEqual(await observed(first.code), { answer: 1, label: null });
  for (const input of [shared, config, path.join(dir, "src"), path.join(dir, "test"), path.join(dir, "package.json"), path.join(dir, "pnpm-lock.yaml")]) {
    assert.ok(first.watched.includes(input), `${input} is watched`);
  }
  writeFileSync(shared, answer(2));
  const second = await load(plugin, file);
  assert.notEqual(second.code, first.code);
  assert.deepEqual(await observed(second.code), { answer: 2, label: null });
  writeFileSync(config, settings("all"));
  const third = await load(plugin, file);
  assert.notEqual(third.code, second.code);
  assert.deepEqual(await observed(third.code), { answer: 2, label: "Ui.Widget.state" });
  const added = path.join(dir, "src/Added.fl");
  writeFileSync(added, "module Added exposing (answer)\n\nanswer : Number\nlet answer = 3\n");
  const fourth = await load(plugin, file);
  assert.notEqual(fourth.code, third.code, "new files change the compiler's input identity");
  rmSync(added);
  assert.equal((await load(plugin, file)).code, third.code, "deletions restore the matching input state");
});

test("concurrent SSR/client loads share only the in-flight real compiler build", async (t) => {
  const dir = fixture(t, "single-flight");
  const file = path.join(dir, "Shared.fl");
  writeFileSync(file, "module Shared exposing (answer)\n\nanswer : Number\nlet answer = 42\n");
  const log = path.join(dir, "calls");
  const compiler = path.join(dir, "flick");
  writeFileSync(compiler, `#!/usr/bin/env node
const { appendFileSync } = require("node:fs");
const { spawnSync } = require("node:child_process");
const args = process.argv.slice(2);
appendFileSync(${JSON.stringify(log)}, args.includes("--inputs") ? "inputs\\n" : "build\\n");
const child = spawnSync(${JSON.stringify(binary)}, args, { stdio: "inherit" });
if (child.error) throw child.error;
process.exit(child.status ?? 1);
`, { mode: 0o755 });
  const plugin = flickVite({ binary: compiler, outDir: path.join(dir, "out") });
  const [ssr, client] = await Promise.all([load(plugin, file), load(plugin, `${file}?import`)]);
  assert.equal(ssr.code, client.code);
  assert.equal((await imported(ssr.code)).answer, 42);
  assert.deepEqual(readFileSync(log, "utf8").trim().split("\n"), ["inputs", "build"]);
  assert.equal((await load(plugin, file)).code, ssr.code);
  assert.deepEqual(readFileSync(log, "utf8").trim().split("\n"), ["inputs", "build", "inputs", "build"]);
});

test("a failed build can be retried after the compiler becomes available", async (t) => {
  const dir = fixture(t, "retry");
  const file = path.join(dir, "Retry.fl");
  writeFileSync(file, 'module Retry exposing (answer)\n\nanswer : String\nlet answer = "ready"\n');
  const compiler = path.join(dir, "flick");
  const plugin = flickVite({ binary: compiler, outDir: path.join(dir, "out") });
  await assert.rejects(load(plugin, file), /cannot run .*set FLICK_BIN/);
  symlinkSync(binary, compiler);
  assert.equal((await imported((await load(plugin, file)).code)).answer, "ready");
});

test("host build and dev profiles override project settings and respect an explicit debug profile", async (t) => {
  const dir = fixture(t, "profile");
  const labels = [];
  for (const [command, profile, configured] of [["build", undefined, "debug"], ["serve", undefined, "release"], ["build", "debug", "release"]]) {
    const root = path.join(dir, `${command}-${profile ?? "default"}`);
    mkdirSync(root);
    writeFileSync(path.join(root, "flick.toml"), `schema-version = 1\n[project]\nkind = "lib"\nsource-dir = "."\n[build]\nprofile = "${configured}"\nlabels = "none"\n`);
    const file = path.join(root, "Profile.fl");
    writeFileSync(file, "module Profile exposing (state)\n\nimport Reactivity.Atom as Atom\n\nstate : Atom.Writable Number\nlet state = Atom.state 1\n");
    const plugin = flickVite({ binary, profile });
    plugin.configResolved({ command, cacheDir: path.join(root, "cache") });
    const built = await imported((await load(plugin, file)).code);
    labels.push(built.state.label?.[0] ?? null);
  }
  assert.deepEqual(labels, [null, "Profile.state", "Profile.state"]);
});

test("a running Astro host reloads external dependencies, configuration, deletions and creations", { timeout: 60_000 }, async (t) => {
  const dir = fixture(t, "host");
  const { file, shared, config, settings, answer } = project(path.join(dir, "project"));
  const site = path.join(dir, "site");
  mkdirSync(path.join(site, "src/pages"), { recursive: true });
  writeFileSync(path.join(site, "src/pages/index.astro"), `---
import { answer, state } from ${JSON.stringify(file)};
const label = state.label?.[0] ?? "none";
---
<output id="answer">{answer}</output><output id="label">{label}</output>
`);
  const require = createRequire(path.join(repo, "docs/package.json"));
  const host = JSON.parse(readFileSync(path.join(repo, "docs/package.json"), "utf8"));
  writeFileSync(path.join(site, "package.json"), JSON.stringify({
    private: true,
    type: "module",
    dependencies: { astro: host.dependencies.astro },
  }));
  symlinkSync(path.join(repo, "docs/node_modules"), path.join(site, "node_modules"), "junction");
  const { dev } = await import(pathToFileURL(require.resolve("astro")));
  const server = await dev({
    root: site,
    configFile: false,
    logLevel: "silent",
    cacheDir: path.join(dir, "astro-cache"),
    vite: { cacheDir: path.join(dir, "vite-cache") },
    integrations: [flick({ binary, profile: "release" })],
    server: { host: "127.0.0.1", port: 0 },
  });
  const url = `http://127.0.0.1:${server.address.port}/`;
  const client = async () => {
    const response = await fetch(new URL(`/@fs/${file.replaceAll(path.sep, "/")}?import`, url));
    const body = await response.text();
    assert.equal(response.status, 200, body);
    assert.match(response.headers.get("content-type"), /(?:text|application)\/javascript/);
    const etag = response.headers.get("etag");
    assert.ok(etag, "the client transform exposes its HTTP cache identity");
    return etag;
  };
  const page = async (answer, label, status = 200) => {
    const deadline = Date.now() + 10_000;
    let response;
    let html;
    do {
      response = await fetch(url);
      html = await response.text();
      if (response.status === status) {
        if (status !== 200) return;
        const document = JSDOM.fragment(html);
        if (document.querySelector("#answer")?.textContent === String(answer)
          && document.querySelector("#label")?.textContent === label) return;
      }
      await delay(25);
    } while (Date.now() < deadline);
    assert.equal(response.status, status, html);
    assert.fail(`expected answer ${answer}, label ${label}: ${html}`);
  };
  try {
    await page(1, "none");
    const initialClient = await client();
    writeFileSync(shared, answer(2));
    await page(2, "none");
    const editedClient = await client();
    assert.notEqual(editedClient, initialClient, "source edits invalidate client transforms");
    writeFileSync(config, settings("all"));
    await page(2, "Ui.Widget.state");
    const configuredClient = await client();
    assert.notEqual(configuredClient, editedClient, "configuration edits invalidate client transforms");
    rmSync(shared);
    await page(undefined, undefined, 500);
    writeFileSync(shared, answer(3));
    await page(3, "Ui.Widget.state");
    await client();
    const nearerConfig = path.join(dir, "project/src/flick.toml");
    writeFileSync(nearerConfig, settings("none", "."));
    await page(3, "none");
    await client();
    rmSync(nearerConfig);
    await page(3, "Ui.Widget.state");
    await client();
  } finally {
    await server.stop();
  }
});
