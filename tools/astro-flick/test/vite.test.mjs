import assert from "node:assert/strict";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { flickVite, moduleName } from "../vite.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../../..");
const binary = path.join(repo, "target/debug/flick");

const load = (plugin, id) => {
  const watched = [];
  const context = {
    addWatchFile: (f) => watched.push(f),
    error: (message) => {
      throw new Error(message);
    },
  };
  return { code: plugin.load.call(context, id), watched };
};

test("a module's name comes from its header", () => {
  assert.equal(moduleName("// c\nmodule App.Main exposing (main)\n"), "App.Main");
  assert.equal(moduleName("let x = 1"), undefined);
});

test("importing a .fl builds it and re-exports the compiled module", () => {
  const dir = path.join(repo, "target/tmp/astro-flick");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const file = path.join(dir, "Greet.fl");
  writeFileSync(file, 'module Greet exposing (greeting)\n\ngreeting : String\nlet greeting = "hi"\n');
  const plugin = flickVite({ binary, outDir: path.join(dir, "out") });
  const first = load(plugin, `${file}?import`);
  const target = /export \* from "(.*)";/.exec(first.code)[1];
  assert.match(readFileSync(target, "utf8"), /export const greeting = "hi"/);
  assert.deepEqual(first.watched, [file]);
  writeFileSync(file, 'module Greet exposing (greeting)\n\ngreeting : String\nlet greeting = "hello"\n');
  const second = load(plugin, file);
  assert.notEqual(second.code, first.code, "an edit builds anew");
  assert.equal(load(plugin, path.join(dir, "x.js")).code, null);
  writeFileSync(file, "module Greet exposing (greeting)\n\ngreeting : String\nlet greeting = 1\n");
  assert.throws(() => load(plugin, file), /FLICK-T003/);
});
