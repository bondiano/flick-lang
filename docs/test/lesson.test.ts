import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { cpSync, mkdirSync, readFileSync, readdirSync, rmSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { lessonCheck } from "../src/playground/lesson-check.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");
const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");
const exercise = path.join(here, "../exercises/signatures/01");

test("signature starter fails and solution compiles, exports, and prints 5", () => {
  const config = readFileSync(path.join(exercise, "check.toml"), "utf8");
  assert.match(config, new RegExp(`^export = "${lessonCheck.exportName}"$`, "m"));
  assert.match(config, new RegExp(`^return_type = "${lessonCheck.returnType}"$`, "m"));
  assert.match(config, new RegExp(`^output = "${lessonCheck.output}"$`, "m"));
  for (const name of ["starter", "solution"]) {
    const dir = path.join(repo, "target/tmp/docs-lesson", name);
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    cpSync(path.join(exercise, `${name}.fl`), path.join(dir, "Main.fl"));
    if (name === "starter") {
      const checked = spawnSync(flick, ["check", "Main.fl", "--diagnostics", "json"], { cwd: dir, encoding: "utf8" });
      assert.notEqual(checked.status, 0);
      assert.match(checked.stderr, /FLICK-T003/);
    } else {
      execFileSync(flick, ["check", "Main.fl", "--warnings-as-errors"], { cwd: dir });
      execFileSync(flick, ["build", "Main.fl", "--emit-dts", "--out-dir", "build"], { cwd: dir });
      const declarations = readdirSync(path.join(dir, "build")).filter((file) => file.endsWith(".d.ts"))
        .map((file) => readFileSync(path.join(dir, "build", file), "utf8")).join("\n");
      assert.match(declarations, /length_of:\s*\([^)]*: string\)\s*=>\s*number/);
      const out = execFileSync("node", [path.join(here, "run-linked.ts"), path.join(dir, "build"), "flick-main.js"], { cwd: here, encoding: "utf8" });
      assert.equal(out.trim(), "5");
    }
  }
});
