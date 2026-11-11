// The multi-file example must use real module loading: standalone Markdown
// checks take imports on trust and cannot establish generative identities.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const repo = fileURLToPath(new URL("../../", import.meta.url));
const flick = process.env.FLICK_BIN ?? path.join(repo, "target/debug/flick");
const env = { ...process.env, FLICK_STD_PATH: path.join(repo, "packages/std") };
const files = ["Contracts", "Argument", "Make", "Instances", "Producer", "Consumer", "Bad"]
  .map(name => `src/App/${name}.fl`);

const example = (locale: string) => {
  const page = path.join(repo, "docs/src/content/docs", locale, "reference/modules.md");
  const markdown = readFileSync(page, "utf8");
  const blocks = [...markdown.matchAll(/```text title="(src\/App\/\w+\.fl)"\n([\s\S]*?)\n```/g)]
    .map(match => [match[1]!, `${match[2]}\n`] as const);
  assert.deepEqual(blocks.map(([file]) => file), files, page);
  return blocks;
};

test("the English and Russian generative parameterized examples have identical source", () => {
  assert.deepEqual(example("ru"), example(""));
});

for (const locale of ["", "ru"]) {
  test(`generative parameterized modules (${locale || "en"}): shared imports pass, separate applications do not mix`, () => {
    const parent = path.join(repo, "target/tmp/docs-parameterized-modules");
    mkdirSync(parent, { recursive: true });
    const dir = mkdtempSync(path.join(parent, "example-"));
    try {
      for (const [file, source] of example(locale)) {
        const target = path.join(dir, file);
        mkdirSync(path.dirname(target), { recursive: true });
        writeFileSync(target, source);
      }
      const check = (file: string) => spawnSync(flick, [
        "check", file, "--offline", "--warnings-as-errors", "--diagnostics", "json",
      ], { cwd: dir, env, encoding: "utf8" });

      const valid = check("src/App/Consumer.fl");
      assert.equal(valid.error, undefined);
      assert.equal(valid.status, 0, valid.stderr);
      assert.equal(valid.stderr, "");

      const invalid = check("src/App/Bad.fl");
      assert.equal(invalid.error, undefined);
      assert.equal(invalid.status, 1, invalid.stderr);
      const diagnostics = invalid.stderr.trim().split("\n").map(line => JSON.parse(line) as {
        code: string; severity: string; message: string;
      });
      assert.deepEqual(diagnostics.map(d => [d.code, d.severity]), [["FLICK-T003", "error"]]);
      assert.match(diagnostics[0]!.message, /App\.Instances\.A\.Token/);
      assert.match(diagnostics[0]!.message, /App\.Instances\.B\.Token/);
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });
}
