//! `flick build --declarations` on a GADT: TypeScript infers what a value
//! evaluates to and rejects the constructions Flick rejects. Needs Node and
//! `pnpm install`: run with `--ignored` (part of `just e2e`).

use crate::common;

use std::fs;
use std::path::Path;
use std::process::Command;

use common::flick_in;

const EXPR: &str = "module Expr exposing (..)

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
";

const CONSUMER: &str = r"import { Add, BoolLit, If, NatLit, eval$ } from './dist/Expr.js';
import type { Expr } from './dist/Expr.js';

type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends (<T>() => T extends Y ? 1 : 2) ? true : false;

const n = eval$(NatLit(1));
const b = eval$(BoolLit(true));
const nIsNumber: Equal<typeof n, number> = true;
const bIsBoolean: Equal<typeof b, boolean> = true;

// @ts-expect-error BoolLit builds Expr<boolean>
const bad: Expr<number> = BoolLit(true);

// @ts-expect-error Add requires two Expr<number> values
Add(NatLit(1))(BoolLit(true));

// @ts-expect-error eval$<number> requires Expr<number>
eval$<number>(BoolLit(true));

const chosen = If(BoolLit(true))(NatLit(1))(Add(NatLit(2))(NatLit(3)));
const numeric: Expr<number> = chosen;
const picked = eval$(chosen);
const pickedIsNumber: Equal<typeof picked, number> = true;

export const describe = (e: Expr<number>): string => {
  switch (e._tag) {
    case 'NatLit#432c78c0cc6d':
      return 'literal';
    case 'Add#7dcc5d5bd789':
      return 'sum';
    case 'If#2c4525a885fe':
      return 'choice';
    default: {
      const rest: never = e;
      return rest;
    }
  }
};

export { nIsNumber, bIsBoolean, bad, numeric, pickedIsNumber };
";

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_gadt_keeps_its_result_indices_in_typescript() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dts_gadt");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").expect("write");
    fs::write(dir.join("src/Expr.fl"), EXPR).expect("write");
    let build = flick_in(&dir, &["build", "--declarations"]);
    assert!(build.status.success(), "{build:?}");
    fs::write(dir.join("consumer.ts"), CONSUMER).expect("write");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    fs::write(
        dir.join("tsconfig.json"),
        serde_json::json!({
            "compilerOptions": {
                "strict": true, "noEmit": true, "module": "nodenext",
                "moduleResolution": "nodenext", "target": "es2022",
                "lib": ["es2022", "dom", "esnext.disposable"], "types": ["node"],
                "typeRoots": [repo.join("runtime/node_modules/@types")],
            },
            "files": ["consumer.ts"],
        })
        .to_string(),
    )
    .expect("write");
    let tsc = Command::new(repo.join("node_modules/.bin/tsc"))
        .arg("-p")
        .arg(&dir)
        .output()
        .expect("tsc");
    let dts = fs::read_to_string(dir.join("dist/Expr.d.ts")).unwrap_or_default();
    assert!(
        tsc.status.success(),
        "{}\n{}\n{dts}",
        String::from_utf8_lossy(&tsc.stdout),
        String::from_utf8_lossy(&tsc.stderr),
    );
    // the tags the switch names are those the JavaScript builds
    let node = Command::new("node")
        .arg("--input-type=module")
        .arg("-e")
        .arg(
            "import { Add, BoolLit, If, NatLit, eval$ } from './dist/Expr.js';\n\
             const e = If(BoolLit(false))(NatLit(1))(Add(NatLit(2))(NatLit(3)));\n\
             console.log(e._tag, e._1._tag, e._2._tag, eval$(e));",
        )
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(
        String::from_utf8_lossy(&node.stdout),
        "If#2c4525a885fe NatLit#432c78c0cc6d Add#7dcc5d5bd789 5\n",
        "{node:?}"
    );
}
