//! `flick build --emit-dts --promise-facade` on a library: the declarations
//! have no `any` and pass `tsc --strict` together with a consumer, and the
//! Promise façade keeps expected failures, defects and imports apart. Needs
//! Node and `pnpm install`: run with `--ignored` (part of `just e2e`).

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::flick_in;

const SHAPES: &str = r#"module Lib.Shapes exposing (..)

type Shape = Circle Number | Rect Number Number

type alias Point = { x : Number; y : Number }
  deriving Schema

type error NotFound = NotFound String

type Tree a = Leaf | Node (Tree a) a (Tree a)

class Describe a =
  describe : a -> String

instance Describe Shape =
  describe : Shape -> String
  let describe shape = "a shape"

area : Shape -> Number
let area shape =
  match shape with
  | Circle r -> r * r
  | Rect w h -> w * h

origin : Point
let origin = { x = 0; y = 0 }

point_json : PureCodec Json Point
let point_json = Codec.json (schema Point)

identity_of : a -> a
let identity_of x = x

described : Describe a => a -> String
let described x = describe x

find : String ->{; NotFound} Number
let find name = if name == "one" then 1 else from_result (Err (NotFound name))

broken : Number ->{} Number
let broken n = n + crash ()

crash : Unit ->{} Number
foreign sync crash from "./crash.mjs" "crash"
  arity 0

later : Number -> Eff none none Number
let later n = task { n + 1 }

maybe : List Number -> Option Number
let maybe xs =
  match xs with
  | [x] -> Some x
  | _ -> None
"#;

const HKT: &str = r"module Lib.Abstract exposing (twice)

twice : Functor f => f Number -> f Number
let twice values = map (fun n -> n * 2) values
";

const CONSUMER: &str = r#"import type { Effect, Option, Result, Schema } from "@flick-lang/runtime";
import { area, identity_of, origin, maybe, find, later, point_json, Circle } from "./dist/Lib/Shapes.js";
import type { Shape, Point, NotFound, Tree } from "./dist/Lib/Shapes.js";
import { find as findPromise } from "./dist/Lib/Shapes.promise.js";

const shape: Shape = Circle(2);
const a: number = area(shape);
const p: Point = origin;
const s: string = identity_of("x");
const m: Option.Option<number> = maybe([1]);
const e: Effect.Effect<number, NotFound, never> = find("one");
const l: Effect.Effect<number, never, never> = later(1);
const r: Promise<Result.Result<number, NotFound>> = findPromise("one");
const t: Tree<number> = { _tag: "Leaf#1145052a8bd3" };
const c: Schema.Codec<Point, unknown> = point_json;
export { a, p, s, m, e, l, r, t, c };
"#;

const RUN: &str = r#"import { find, broken } from "./dist/Lib/Shapes.promise.js";
import { crashes } from "./src/Lib/crash.mjs";

const ok = await find("one");
const failed = await find("two");
let rejected;
try {
  await broken(1);
} catch (error) {
  rejected = `${error.name}: ${error.message}`;
}
console.log(JSON.stringify({ ok: ok._tag, value: ok.success, failed: failed._tag, error: failed.failure._tag, rejected, crashes: crashes() }));
"#;

fn project(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/tmp/dts_command")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/Lib")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(
        dir.join("package.json"),
        "{ \"name\": \"@acme/shapes\", \"type\": \"module\" }\n",
    )
    .expect("write");
    fs::write(dir.join("src/Lib/Shapes.fl"), SHAPES).expect("write");
    fs::write(
        dir.join("src/Lib/crash.mjs"),
        "let count = 0;\nexport const crash = () => { count++; throw new Error(\"crashed\"); };\nexport const crashes = () => count;\n",
    )
    .expect("write");
    dir
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn declarations_pass_tsc_and_the_facade_separates_outcomes() {
    let dir = project("library");
    let out = flick_in(&dir, &["build", "--emit-dts", "--promise-facade"]);
    assert!(out.status.success(), "{out:?}");
    let dts = fs::read_to_string(dir.join("dist/Lib/Shapes.d.ts")).expect("d.ts");
    assert!(!dts.contains("any"), "{dts}");
    assert!(
        dts.contains("export declare const area: (arg: Shape) => number;"),
        "{dts}"
    );
    fs::write(dir.join("consumer.ts"), CONSUMER).expect("write");
    fs::write(
        dir.join("tsconfig.json"),
        r#"{ "compilerOptions": { "strict": true, "noEmit": true, "module": "nodenext", "moduleResolution": "nodenext", "target": "es2022", "skipLibCheck": true, "types": [] }, "files": ["consumer.ts"] }"#,
    )
    .expect("write");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let tsc = Command::new(repo.join("node_modules/.bin/tsc"))
        .arg("-p")
        .arg(&dir)
        .output()
        .expect("tsc");
    assert!(
        tsc.status.success(),
        "{}",
        String::from_utf8_lossy(&tsc.stdout)
    );
    fs::write(dir.join("run.mjs"), RUN).expect("write");
    let node = Command::new("node")
        .arg("run.mjs")
        .current_dir(&dir)
        .output()
        .expect("node");
    let text = String::from_utf8_lossy(&node.stdout);
    assert!(node.status.success(), "{node:?}");
    assert!(
        text.contains(
            r#""ok":"Success","value":1,"failed":"Failure","error":"NotFound#14b4b8bdcbdc""#
        ),
        "{text}"
    );
    assert!(
        text.contains(r#""rejected":"RuntimeFailure: die error crashed""#),
        "{text}"
    );
    // importing the library ran nothing: only the façade call did
    assert!(text.contains(r#""crashes":1"#), "{text}");
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn an_abstract_higher_kinded_export_has_no_declaration() {
    let dir = project("abstract");
    fs::write(dir.join("src/Lib/Abstract.fl"), HKT).expect("write");
    let out = flick_in(&dir, &["build", "--emit-dts"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-F004"), "{stderr}");
    assert!(
        stderr.contains("`Lib.Abstract.twice` has no TypeScript form"),
        "{stderr}"
    );
    // a package with declarations refuses the same export before any tarball
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n\n[build]\nemit-dts = true\n",
    )
    .expect("write");
    let pack = flick_in(&dir, &["pack", "--out-dir", "artifacts"]);
    assert_eq!(pack.status.code(), Some(1), "{pack:?}");
    assert!(String::from_utf8_lossy(&pack.stderr).contains("FLICK-F004"));
    assert!(!dir.join("artifacts").exists());
}
