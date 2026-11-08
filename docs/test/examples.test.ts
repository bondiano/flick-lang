// The playground's gallery: each example does what its entry in the index says, with the real compiler.
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.join(here, "../..");
const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");
const examples = path.join(here, "../src/playground/examples");

interface Entry {
  readonly id: string;
  readonly en: string;
  readonly ru: string;
  /** `run`, `ui`, or `fail` and the one code its errors have. */
  readonly expect: string;
}

const index = JSON.parse(readFileSync(path.join(examples, "index.json"), "utf8")) as {
  groups: Array<{ id: string; en: string; ru: string; examples: Array<Entry> }>;
};
const entries = index.groups.flatMap((group) => group.examples);

test("the index and the example files match one to one", () => {
  const files = readdirSync(examples).filter((file) => file.endsWith(".fl")).map((file) => file.slice(0, -3));
  assert.deepEqual(entries.map((entry) => entry.id).sort(), files.sort());
  for (const entry of entries) {
    assert.equal(existsSync(path.join(examples, `${entry.id}.stdout`)), entry.expect === "run", entry.id);
    assert.ok(entry.en !== "" && entry.ru !== "", entry.id);
  }
});

for (const entry of entries) {
  test(`example ${entry.id}: ${entry.expect}`, () => {
    const dir = path.join(repo, "target/tmp/docs-examples", entry.id);
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(dir, { recursive: true });
    cpSync(path.join(examples, `${entry.id}.fl`), path.join(dir, "Main.fl"));
    if (entry.expect.startsWith("fail ")) {
      const checked = spawnSync(flick, ["check", "Main.fl", "--diagnostics", "json"], { cwd: dir, encoding: "utf8" });
      assert.notEqual(checked.status, 0, entry.id);
      const codes = checked.stderr
        .split("\n")
        .filter((line) => line.startsWith("{"))
        .map((line) => JSON.parse(line) as { code: string; severity: string })
        .filter((event) => event.severity === "error")
        .map((event) => event.code);
      assert.ok(codes.length > 0, checked.stderr);
      assert.deepEqual([...new Set(codes)], [entry.expect.slice("fail ".length)], checked.stderr);
      return;
    }
    execFileSync(flick, ["check", "Main.fl", "--warnings-as-errors"], { cwd: dir });
    for (const profile of ["debug", "release"]) {
      execFileSync(flick, ["build", "Main.fl", "--profile", profile, "--out-dir", profile], { cwd: dir });
      if (entry.expect !== "run") continue;
      const out = execFileSync("node", [path.join(here, "run-linked.ts"), path.join(dir, profile), "flick-main.js"], {
        cwd: here,
        encoding: "utf8",
      });
      assert.equal(out, readFileSync(path.join(examples, `${entry.id}.stdout`), "utf8"), `${entry.id}, ${profile}`);
    }
  });
}
