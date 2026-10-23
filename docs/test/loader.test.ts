import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { packages, resolve } from "../src/playground/loader.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");

test("paths resolve like the output directory", () => {
  assert.equal(resolve("App/Main.js", "../_flick/List.js"), "_flick/List.js");
  assert.equal(resolve("main.js", "./App/Main.js"), "App/Main.js");
});

test("package discovery follows reachable module imports", () => {
  assert.deepEqual(packages({ "main.js": 'import "@flick-lang/runtime";\nimport "./a.js";', "a.js": 'import "b";' }, "main.js"), [
    "@flick-lang/runtime",
    "b",
  ]);
});

test("a linked build runs as `flick build` output does", () => {
  const dir = path.join(repo, "target/tmp/docs-loader");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  writeFileSync(
    path.join(dir, "Demo.fl"),
    "module Demo exposing (main)\n\nmain : Unit ->{; AppError} Unit\nlet main () =\n  Platform.stdout_line (show (List.map (fun x -> x * 2) [1, 2, 3]))\n",
  );
  // A distinct fixture name avoids Main.js/main.js collisions on case-insensitive filesystems.
  execFileSync(path.join(repo, "target/debug/flick"), ["build", "Demo.fl", "--out-dir", "out"], { cwd: dir });
  const out = execFileSync("node", [path.join(here, "run-linked.ts"), path.join(dir, "out"), "main.js"], { cwd: here, encoding: "utf8" });
  assert.equal(out, "[2, 4, 6]\n");
});
