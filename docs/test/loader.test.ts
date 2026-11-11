import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { packages, resolve } from "../src/playground/loader.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");
const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");

test("paths resolve like the output directory", () => {
  assert.equal(resolve("App/Main.js", "../_flick/List.js"), "_flick/List.js");
  assert.equal(resolve("flick-main.js", "./App/Main.js"), "App/Main.js");
});

test("package discovery follows reachable module imports", () => {
  assert.deepEqual(packages({ "flick-main.js": 'import "@flick-lang/effect-ts";\nimport "./a.js";', "a.js": 'import "b";' }, "flick-main.js"), [
    "@flick-lang/effect-ts",
    "b",
  ]);
});

test("a string that reads like an import is not linked", () => {
  const code = 'import "./a.js";\nconst s = "taken from " + n + " to ";\nexport const t = "export { x } from " + s;';
  assert.deepEqual(packages({ "flick-main.js": code, "a.js": 'export * from "b";' }, "flick-main.js"), ["b"]);
  const dir = path.join(repo, "target/tmp/docs-loader-string");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  writeFileSync(
    path.join(dir, "Main.fl"),
    'module Main exposing (main)\n\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line "from \\{1} to \\{2}"\n',
  );
  execFileSync(flick, ["build", "Main.fl", "--out-dir", "out"], { cwd: dir });
  const out = execFileSync("node", [path.join(here, "run-linked.ts"), path.join(dir, "out"), "flick-main.js"], { cwd: here, encoding: "utf8" });
  assert.equal(out, "from 1 to 2\n");
});

test("a linked build runs as `flick build` output does", () => {
  const dir = path.join(repo, "target/tmp/docs-loader");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  writeFileSync(
    path.join(dir, "Main.fl"),
    "module Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () =\n  Platform.stdout_line (show (List.map (fun x -> x * 2) [1, 2, 3]))\n",
  );
  execFileSync(flick, ["build", "Main.fl", "--out-dir", "out"], { cwd: dir });
  const out = execFileSync("node", [path.join(here, "run-linked.ts"), path.join(dir, "out"), "flick-main.js"], { cwd: here, encoding: "utf8" });
  assert.equal(out, "[2, 4, 6]\n");
});

test("source-map text survives a compiled and linked release build", () => {
  const dir = path.join(repo, "target/tmp/docs-loader-source-map");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  const text = "//# sourceMappingURL=payload";
  writeFileSync(
    path.join(dir, "Main.fl"),
    `module Main exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () = Platform.stdout_line ${JSON.stringify(text)}\n`,
  );
  execFileSync(flick, ["build", "Main.fl", "--profile", "release", "--out-dir", "out"], { cwd: dir });
  const out = execFileSync("node", [path.join(here, "run-linked.ts"), path.join(dir, "out"), "flick-main.js"], { cwd: here, encoding: "utf8" });
  assert.equal(out, `${text}\n`);
});
