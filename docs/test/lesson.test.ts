import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { loadExercise } from "../exercises/load.ts";
import { evaluateExercise, type ExerciseOperations } from "../src/playground/exercise.ts";
import { errorCodes } from "./helpers/verify-example.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");
const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");
const exerciseIds = readdirSync(path.join(repo, "docs/exercises"), { recursive: true, encoding: "utf8" }).map((name) => name.replaceAll("\\", "/")).filter((name) => name.endsWith("/check.toml")).map((name) => name.slice(0, -"/check.toml".length)).sort();
const nativeOperations = (dir: string, starterError = "FLICK-T003", rt = false): ExerciseOperations => ({
  check: async () => {
    const checked = spawnSync(flick, ["check", "Main.fl", "--diagnostics", "json"], { cwd: dir, encoding: "utf8" });
    if (checked.error) throw checked.error;
    if (checked.status !== 0) {
      assert.deepEqual(errorCodes(checked.stderr), [starterError], checked.stderr);
      return [{ severity: "error" }];
    }
    return [];
  },
  compile: async () => {
    execFileSync(flick, ["check", "Main.fl", "--warnings-as-errors"], { cwd: dir });
    execFileSync(flick, ["build", "Main.fl", "--emit-dts", "--out-dir", "build", ...(rt ? ["--rt"] : [])], { cwd: dir });
    const output = path.join(dir, "build");
    const dts = readFileSync(path.join(output, "Main.d.ts"), "utf8");
    const names = readdirSync(output, { recursive: true, encoding: "utf8" }).map((name) => name.replaceAll("\\", "/")).filter((name) => name.endsWith(".d.ts") && name !== "Main.d.ts");
    const declarations = Object.fromEntries(names.map((name) => [name, readFileSync(path.join(output, name), "utf8")]));
    return { dts, declarations, declarationEntry: "Main.d.ts" };
  },
  run: async () => ({ status: "exit", code: 0, lines: [{ stream: "stdout", text: execFileSync("node", [path.join(here, "run-linked.ts"), path.join(dir, "build"), "flick-main.js"], { cwd: here, encoding: "utf8", timeout: 10_000 }) }] }),
});
for (const id of exerciseIds) {
  test(`${id}: starter fails, solution passes, compilable broken answer fails`, async () => {
    const exercise = loadExercise(id);
    for (const name of ["starter", "solution", "broken"]) {
      const dir = path.join(repo, "target/tmp/docs-lesson", id, name);
      rmSync(dir, { recursive: true, force: true });
      mkdirSync(dir, { recursive: true });
      writeFileSync(path.join(dir, "Main.fl"), readFileSync(path.join(repo, "docs/exercises", id, `${name}.fl`)));
      const result = await evaluateExercise(exercise.check, nativeOperations(dir, exercise.starter_error));
      assert.equal(result, name === "starter" ? "fix-errors" : name === "broken" ? "output" : "passed");
    }
  });
}

test("native declarations resolve a primitive String re-export", async () => {
  const dir = path.join(repo, "target/tmp/docs-lesson/reexport");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(dir, { recursive: true });
  writeFileSync(path.join(dir, "Main.fl"), 'module Main exposing (main, length)\nimport String exposing (length)\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (length "Flick"))\n');
  assert.equal(await evaluateExercise({ export: "length", argument_types: ["string"], return_type: "number", output: "5" }, nativeOperations(dir)), "passed");
});

test("running exercise fixtures pass the same evaluator with the native rt emitter", async () => {
  const id = "effects/01";
  const exercise = loadExercise(id);
  assert.equal(exercise.check.runs, true);
  for (const name of ["starter", "solution", "broken"]) {
    const dir = path.join(repo, "target/tmp/docs-lesson-rt", name);
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    writeFileSync(path.join(dir, "Main.fl"), readFileSync(path.join(repo, "docs/exercises", id, `${name}.fl`)));
    assert.equal(await evaluateExercise(exercise.check, nativeOperations(dir, exercise.starter_error, true)),
      name === "starter" ? "fix-errors" : name === "broken" ? "output" : "passed");
    if (name === "starter") {
      await assert.rejects(() => nativeOperations(dir, "FLICK-T003").check(), /FLICK-E002/);
    }
  }
});
