// The Content Mapper under the pinned TypeScript: a project in the
// repository's target directory, so `@flick-lang/runtime` resolves, whose
// TypeScript imports Flick modules and is checked by `tsc --runExternalCode`.
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

const repo = join(import.meta.dirname, "../../..");

const greeting = `module Lib.Greeting exposing (greet, badge, Visit, visits)

import Lib.Names exposing (Badge(..))

type alias Visit = { name : String; count : Int }

badge : String -> Badge
let badge name = Badge name

greet : Badge -> String
let greet b =
  match b with
  | Badge name -> "Hello, " ++ name

visits : List Visit
let visits = [{ name = "Ada"; count = 1 }]
`;

const names = `module Lib.Names exposing (Badge(..))

type Badge = Badge String
`;

const expr = `module Lib.Expr exposing (..)

type Expr a =
  | NatLit Int : Expr Int
  | BoolLit Bool : Expr Bool
  | Add (Expr Int) (Expr Int) : Expr Int
  | If (Expr Bool) (Expr a) (Expr a) : Expr a

eval : forall a. Expr a -> a
let rec eval e =
  match e with
  | NatLit n -> n
  | BoolLit b -> b
  | Add x y -> eval x + eval y
  | If c t f -> if eval c then eval t else eval f
`;

const flick = () => {
  if (process.env.FLICK_BIN) return process.env.FLICK_BIN;
  execFileSync("cargo", ["build", "-q", "-p", "flick"], { cwd: repo, stdio: "inherit" });
  return join(repo, "target/debug/flick");
};

/** `tsc --runExternalCode` over a project whose `main.ts` is `main`: its exit code and output. */
const check = (bin, main) => {
  const dir = join(repo, "target/tmp/content_mapper");
  rmSync(dir, { recursive: true, force: true });
  mkdirSync(join(dir, "src/Lib"), { recursive: true });
  mkdirSync(join(dir, "node_modules/@flick-lang"), { recursive: true });
  symlinkSync(join(repo, "tools/content-mapper"), join(dir, "node_modules/@flick-lang/content-mapper"));
  writeFileSync(join(dir, "flick.toml"), 'schema-version = 1\n\n[project]\nkind = "lib"\nsource-dir = "src"\n');
  writeFileSync(join(dir, "src/Lib/Greeting.fl"), greeting);
  writeFileSync(join(dir, "src/Lib/Names.fl"), names);
  writeFileSync(join(dir, "src/Lib/Expr.fl"), expr);
  writeFileSync(join(dir, "main.ts"), main);
  writeFileSync(
    join(dir, "tsconfig.json"),
    JSON.stringify({
      compilerOptions: { strict: true, noEmit: true, module: "esnext", moduleResolution: "bundler", target: "esnext", skipLibCheck: true },
      files: ["main.ts"],
      contentMappers: [{ package: "@flick-lang/content-mapper", extensions: [".fl"] }],
    }),
  );
  const tsc = spawnSync(join(repo, "node_modules/.bin/tsc"), ["-p", dir, "--runExternalCode"], {
    encoding: "utf8",
    env: { ...process.env, FLICK_BIN: bin },
  });
  return { status: tsc.status, output: tsc.stdout + tsc.stderr };
};

test("TypeScript checks its calls of a Flick module against the module's types", () => {
  const bin = flick();
  const ok = check(
    bin,
    `import { badge, greet, visits } from "./src/Lib/Greeting.fl";
const said: string = greet(badge("Ada"));
const first: string = visits[0]!.name;
export { said, first };
`,
  );
  assert.equal(ok.status, 0, ok.output);
  const wrong = check(bin, `import { greet } from "./src/Lib/Greeting.fl";\nexport const said = greet("Ada");\n`);
  assert.notEqual(wrong.status, 0);
  assert.match(wrong.output, /main\.ts\(2,\d+\): error TS2345/);
});

test("a GADT's cases keep the index its constructors fix", () => {
  const ok = check(
    flick(),
    `import { Add, BoolLit, If, NatLit, eval$ } from "./src/Lib/Expr.fl";
import type { Expr } from "./src/Lib/Expr.fl";
const n: number = eval$(If(BoolLit(true))(NatLit(1))(Add(NatLit(2))(NatLit(3))));
const b: boolean = eval$(BoolLit(true));
// @ts-expect-error BoolLit builds Expr<boolean>
const bad: Expr<number> = BoolLit(true);
// @ts-expect-error Add requires two Expr<number> values
Add(NatLit(1))(BoolLit(true));
// @ts-expect-error eval$<number> requires Expr<number>
eval$<number>(BoolLit(true));
export { n, b, bad };
`,
  );
  assert.equal(ok.status, 0, ok.output);
});
