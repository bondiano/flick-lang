import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { loadExercise } from "../exercises/load.ts";
import { evaluateExercise } from "../src/playground/exercise.ts";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");
const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");
const exerciseIds = readdirSync(path.join(repo, "docs/exercises"), { recursive: true, encoding: "utf8" }).map((name) => name.replaceAll("\\", "/")).filter((name) => name.endsWith("/check.toml")).map((name) => name.slice(0, -"/check.toml".length)).sort();
for (const id of exerciseIds) {
  test(`${id}: starter fails, solution passes, compilable broken answer fails`, async () => {
    const exercise = loadExercise(id);
    for (const name of ["starter", "solution", "broken"]) {
      const dir = path.join(repo, "target/tmp/docs-lesson", id, name);
      rmSync(dir, { recursive: true, force: true });
      mkdirSync(dir, { recursive: true });
      writeFileSync(path.join(dir, "Main.fl"), readFileSync(path.join(repo, "docs/exercises", id, `${name}.fl`)));
      const result = await evaluateExercise(exercise.check, {
        check: async () => {
          const checked = spawnSync(flick, ["check", "Main.fl", "--diagnostics", "json"], { cwd: dir, encoding: "utf8" });
          if (checked.error) throw checked.error;
          if (checked.status !== 0) {
            assert.match(checked.stderr, /FLICK-T003/);
            return [{ severity: "error" }];
          }
          return [];
        },
        compile: async () => {
          execFileSync(flick, ["check", "Main.fl", "--warnings-as-errors"], { cwd: dir });
          execFileSync(flick, ["build", "Main.fl", "--emit-dts", "--out-dir", "build"], { cwd: dir });
          return { dts: readdirSync(path.join(dir, "build")).filter((file) => file.endsWith(".d.ts")).map((file) => readFileSync(path.join(dir, "build", file), "utf8")).join("\n") };
        },
        run: async () => ({ status: "exit", code: 0, lines: [{ stream: "stdout", text: execFileSync("node", [path.join(here, "run-linked.ts"), path.join(dir, "build"), "flick-main.js"], { cwd: here, encoding: "utf8", timeout: 10_000 }) }] }),
      });
      assert.equal(result, name === "starter" ? "fix-errors" : name === "broken" ? "output" : "passed");
    }
  });
}
