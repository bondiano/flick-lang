// Native tour examples use real project entry points, never a live network or database.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import type { Lesson } from "../../tour/load.ts";
import { flick, repo } from "./verify-example.ts";

export const verifyLocalLesson = (lesson: Lesson): void => {
  const check = lesson.local;
  assert.ok(check);
  assert.ok(lesson.file);
  const parent = path.join(repo, "target/tmp/docs-tour-local");
  mkdirSync(parent, { recursive: true });
  const dir = mkdtempSync(path.join(parent, `${lesson.id}-`));
  const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/KEY|SECRET|TOKEN|PASSWORD/i.test(key)));
  env.FLICK_STD_PATH = path.join(repo, "packages/std");
  try {
    writeFileSync(path.join(dir, "flick.toml"), 'schema-version = 1\n[project]\nkind = "lib"\n[tools]\n"flick/go" = ["go"]\n');
    writeFileSync(path.join(dir, "package.json"), JSON.stringify({
      name: "@flick-tour/local",
      private: true,
      ...(check.adapter ? { dependencies: { "@flick-lang/effect-ts": "workspace:*" } } : {}),
    }) + "\n");
    mkdirSync(path.join(dir, "src"), { recursive: true });
    const source = check.source ?? (check.command === "test" ? "test" : check.command === "prove" ? "proofs" : "src");
    mkdirSync(path.join(dir, source), { recursive: true });
    cpSync(lesson.file, path.join(dir, source, "Main.fl"));
    for (const [destination, file] of Object.entries(check.files ?? {})) {
      assert.ok(!path.isAbsolute(destination) && !destination.split("/").includes(".."), destination);
      mkdirSync(path.dirname(path.join(dir, destination)), { recursive: true });
      cpSync(path.join(repo, "docs", file), path.join(dir, destination));
    }
    const run = (args: ReadonlyArray<string>) => {
      const out = spawnSync(flick, [...args], { cwd: dir, env, encoding: "utf8", timeout: 60_000 });
      if (out.error) throw out.error;
      assert.equal(out.signal, null, out.stderr);
      return out;
    };
    for (const target of check.targets ?? ["js"]) {
      const args = [check.command,
        ...(check.command === "check" || check.command === "run" ? [`${source}/Main.fl`] : []),
        ...(check.command === "check" ? ["--warnings-as-errors"] : []),
        ...(check.command === "run" ? ["--entry", "Main"] : []),
        ...(check.command === "test" || check.command === "run" ? ["--target", target, ...(target === "js" ? ["--runtime", "rt"] : [])] : []),
        ...(check.flags ?? [])];
      const out = run(args);
      assert.equal(out.status, check.exit, `${lesson.id}: ${out.stdout}\n${out.stderr}`);
      if (check.command === "prove") assert.ok(!out.stdout.includes("no proofs:"), `${lesson.id}: no proof modules were checked`);
      for (const line of check.contains) assert.ok(out.stdout.includes(line), `${lesson.id}: missing ${line}\n${out.stdout}\n${out.stderr}`);
      if (check.printed) assert.deepEqual(out.stdout.split("\n").filter((line) => line.startsWith("hook:")), check.printed);
      if (check.listed) {
        const listed = run(["test", "--list", "--target", target]);
        assert.equal(listed.status, 0, listed.stderr);
        assert.deepEqual(listed.stdout.trimEnd().split("\n"), check.listed);
      }
      if (check.forbidOnly) {
        const forbidden = run(["test", "--forbid-only", "--target", target]);
        assert.equal(forbidden.status, 1, forbidden.stderr);
        assert.ok(forbidden.stdout.includes("--forbid-only: 1 test is marked only"), forbidden.stdout);
      }
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};
