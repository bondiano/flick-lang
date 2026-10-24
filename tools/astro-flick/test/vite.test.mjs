import assert from "node:assert/strict";
import { mkdirSync, readFileSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

import { flickVite, moduleName } from "../vite.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../../..");
const binary = path.join(repo, "target/debug/flick");

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

test("a module's name comes from its header", () => {
  assert.equal(moduleName("// c\nmodule App.Main exposing (main)\n"), "App.Main");
  assert.equal(moduleName("let x = 1"), undefined);
});

test("compilation keeps Vite responsive and rebuilds edited sources", async () => {
  const dir = path.join(repo, "target/tmp/astro-flick");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const file = path.join(dir, "Greet.fl");
  writeFileSync(file, 'module Greet exposing (greeting)\n\ngreeting : String\nlet greeting = "hi"\n');
  const plugin = flickVite({ binary, outDir: path.join(dir, "out") });
  let ticked = false;
  setImmediate(() => { ticked = true; });
  const first = await load(plugin, `${file}?import`);
  assert.equal(ticked, true, "compiling must not block Vite's event loop");
  const target = /export \* from "(.*)";/.exec(first.code)[1];
  assert.equal((await import(pathToFileURL(target))).greeting, "hi");
  assert.deepEqual(first.watched, [file]);
  writeFileSync(file, 'module Greet exposing (greeting)\n\ngreeting : String\nlet greeting = "hello"\n');
  const second = await load(plugin, file);
  assert.notEqual(second.code, first.code, "an edit builds anew");
  const edited = /export \* from "(.*)";/.exec(second.code)[1];
  assert.equal((await import(pathToFileURL(edited))).greeting, "hello");
  assert.equal((await load(plugin, path.join(dir, "x.js"))).code, null);
  writeFileSync(file, "module Greet exposing (greeting)\n\ngreeting : String\nlet greeting = 1\n");
  await assert.rejects(load(plugin, file), /FLICK-T003/);
});

test("a failed build can be retried after the compiler becomes available", async (t) => {
  const dir = path.join(repo, "target/tmp/astro-flick-retry");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const file = path.join(dir, "Retry.fl");
  writeFileSync(file, 'module Retry exposing (answer)\n\nanswer : String\nlet answer = "ready"\n');
  const compiler = path.join(dir, "flick");
  const plugin = flickVite({ binary: compiler, outDir: path.join(dir, "out") });
  await assert.rejects(load(plugin, file), /cannot run .*set FLICK_BIN/);
  symlinkSync(binary, compiler);
  const { code } = await load(plugin, file);
  const target = /export \* from "(.*)";/.exec(code)[1];
  assert.equal((await import(pathToFileURL(target))).answer, "ready");
});

test("`vite build` compiles in release, the dev server in debug", async (t) => {
  const dir = path.join(repo, "target/tmp/astro-flick-profile");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const compiler = path.join(dir, "flick");
  writeFileSync(compiler, '#!/bin/sh\necho "$@" >> "$(dirname "$0")/args"\n', { mode: 0o755 });
  const profiles = [];
  for (const command of ["build", "serve"]) {
    const file = path.join(dir, `${command}.fl`);
    writeFileSync(file, "module Main exposing (x)\n\nx : Number\nlet x = 1\n");
    const plugin = flickVite({ binary: compiler });
    plugin.configResolved({ command, cacheDir: path.join(dir, command) });
    await load(plugin, file);
    const args = readFileSync(path.join(dir, "args"), "utf8").trim().split("\n").at(-1).split(" ");
    profiles.push(args[args.indexOf("--profile") + 1]);
  }
  assert.deepEqual(profiles, ["release", "debug"]);
});
