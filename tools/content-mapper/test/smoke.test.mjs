// Smoke test of the adapter on the pinned typescript-go: a project of the
// repository's target directory, so that `@flick-lang/runtime` resolves.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

import { TYPESCRIPT, open, project, toFlick, toProjection, typescriptVersion } from "../index.mjs";

const repo = join(import.meta.dirname, "../../..");

const source = `module Lib.Greeting exposing (greet, Visit, visits)

type alias Visit = { name : String; count : Number }

greet : String -> String
let greet name = "Hello, " ++ name

visits : List Visit
let visits = [{ name = "Ada"; count = 1 }]
`;

test("the projection type-checks on the pinned typescript-go and maps back", () => {
  assert.equal(typescriptVersion(), TYPESCRIPT);
  execFileSync("cargo", ["build", "-q", "-p", "flick"], { cwd: repo, stdio: "inherit" });
  const flick = join(repo, "target/debug/flick");
  const dir = join(repo, "target/tmp/content_mapper");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(join(dir, "src/Lib"), { recursive: true });
  writeFileSync(join(dir, "flick.toml"), 'schema-version = 1\n\n[project]\nkind = "lib"\n');
  writeFileSync(join(dir, "src/Lib/Greeting.fl"), source);
  const projected = project(flick, dir, "Lib.Greeting");
  assert.equal(projected.module, "Lib.Greeting");
  const program = open([projected], dir);
  try {
    assert.deepEqual(program.diagnostics(projected), []);
    const greet = Buffer.byteLength(source.slice(0, source.indexOf("let greet") + 4));
    assert.match(program.typeAt(projected, greet) ?? "", /string/);
    const back = toFlick(projected, toProjection(projected, greet));
    assert.equal(source.slice(back.start, back.end), "greet");
    assert.ok(back.exact);
  } finally {
    program.close();
  }
});
