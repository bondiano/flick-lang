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

type Shape = Circle Float | Rect Float Float

type alias Point = { x : Float; y : Float }
  deriving Schema

type error NotFound = NotFound String

type Tree a = Leaf | Node (Tree a) a (Tree a)

class Describe a =
  describe : a -> String

instance Describe Shape =
  describe : Shape -> String
  let describe shape = "a shape"

area : Shape -> Float
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

find : String ->{; NotFound} Float
let find name = if name == "one" then 1 else from_result (Err (NotFound name))

broken : Float ->{} Float
let broken n = n + crash ()

crash : Unit ->{} Float
foreign sync crash from "./crash.mjs" "crash"
  arity 0

later : Float -> Eff none none Float
let later n = task { n + 1 }

waiting : Float ->{} Float
let waiting n =
  scope {
    ignore (Resource.acquire_release (task { n }) (fun _ -> cleanup ()))
    pause n
  }

pause : Float ->{} Float
foreign async pause from "./crash.mjs" "pause"
  arity 1
  cancel signal

cleanup : Unit ->{} Unit
foreign async cleanup from "./crash.mjs" "cleanup"
  arity 0

maybe : List Float -> Option Float
let maybe xs =
  match xs with
  | [x] -> Some x
  | _ -> None
"#;

const HKT: &str = r"module Lib.Abstract exposing (twice)

twice : Functor f => f Float -> f Float
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
const r: Promise<Result.Result<number, NotFound>> = findPromise("one", { signal: new AbortController().signal });
const t: Tree<number> = { _tag: "Leaf#1145052a8bd3" };
const c: Schema.Codec<Point, unknown> = point_json;
export { a, p, s, m, e, l, r, t, c };
// @ts-expect-error the parameter must be a Shape
area("not a shape");
// @ts-expect-error the result is a number, not an unconstrained value
const wrongArea: string = area(shape);
"#;

const RUN: &str = r#"import assert from "node:assert/strict";
import { getEventListeners } from "node:events";
import { find, broken, waiting } from "./dist/Lib/Shapes.promise.js";
import { crashes, awaiting, cleaning, releaseCleanup, lifecycle } from "./src/Lib/crash.mjs";

const ok = await find("one");
const failed = await find("two");
let rejected;
try {
  await broken(1);
} catch (error) {
  rejected = `${error.name}: ${error.message}`;
}

const preAborted = new AbortController();
preAborted.abort();
await assert.rejects(broken(1, { signal: preAborted.signal }), { name: "RuntimeFailure" });
assert.equal(crashes(), 1, "a pre-aborted facade must not call the foreign function");
assert.equal(getEventListeners(preAborted.signal, "abort").length, 0);

const shared = new AbortController();
for (let i = 0; i < 3; i++) {
  await find("one", { signal: shared.signal });
  assert.equal(getEventListeners(shared.signal, "abort").length, 0);
}

const controller = new AbortController();
let settled = false;
const pending = waiting(7, { signal: controller.signal });
const cancelled = assert.rejects(pending, { name: "RuntimeFailure" }).then(() => { settled = true; });
await awaiting;
assert.equal(getEventListeners(controller.signal, "abort").length, 1);
controller.abort();
await cleaning;
assert.equal(settled, false, "the facade must wait for its Flick resource finalizer");
assert.deepEqual(lifecycle, ["started", "aborted", "cleanup started"]);
releaseCleanup();
await cancelled;
assert.deepEqual(lifecycle, ["started", "aborted", "cleanup started", "cleanup ended"]);
assert.equal(getEventListeners(controller.signal, "abort").length, 0);

console.log(JSON.stringify({ ok: ok._tag, value: ok.success, failed: failed._tag, error: failed.failure._tag, rejected, crashes: crashes() }));
"#;

const CRASH: &str = r#"let count = 0;
export const crash = () => { count++; throw new Error("crashed"); };
export const crashes = () => count;
const started = Promise.withResolvers();
const closing = Promise.withResolvers();
const released = Promise.withResolvers();
export const awaiting = started.promise;
export const cleaning = closing.promise;
export const releaseCleanup = () => released.resolve();
export const lifecycle = [];
export const pause = (n, signal) => new Promise((resolve) => {
  lifecycle.push("started");
  started.resolve();
  signal.addEventListener("abort", () => {
    lifecycle.push("aborted");
    resolve(n);
  }, { once: true });
});
export const cleanup = async () => {
  lifecycle.push("cleanup started");
  closing.resolve();
  await released.promise;
  lifecycle.push("cleanup ended");
};
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
    fs::write(dir.join("src/Lib/crash.mjs"), CRASH).expect("write");
    dir
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn declarations_pass_tsc_and_the_facade_separates_outcomes() {
    let dir = project("library");
    let out = flick_in(&dir, &["build", "--emit-dts", "--promise-facade"]);
    assert!(out.status.success(), "{out:?}");
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

#[test]
#[ignore = "needs pnpm install"]
fn dotted_and_underscored_module_types_have_distinct_namespaces() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dts_module_aliases");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/Foo")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").expect("write");
    for module in ["Foo.Bar", "Foo_Bar"] {
        fs::write(
            dir.join("src")
                .join(format!("{}.fl", module.replace('.', "/"))),
            format!("module {module} exposing (Token(..))\ntype Token = Token Float | Empty\n"),
        )
        .expect("write");
    }
    fs::write(
        dir.join("src/Uses.fl"),
        "module Uses exposing (dot, underscore, pair)\n\
         import Foo.Bar\nimport Foo_Bar\n\
         dot : Foo.Bar.Token\nlet dot = Foo.Bar.Token 1\n\
         underscore : Foo_Bar.Token\nlet underscore = Foo_Bar.Token 2\n\
         pair : (Foo.Bar.Token, Foo_Bar.Token)\nlet pair = (dot, underscore)\n",
    )
    .expect("write");
    let build = flick_in(&dir, &["build", "--emit-dts"]);
    assert!(build.status.success(), "{build:?}");
    fs::write(
        dir.join("consumer.ts"),
        "import { dot, underscore, pair } from './dist/Uses.js';\n\
         import type { Token as Dot } from './dist/Foo/Bar.js';\n\
         import type { Token as Underscore } from './dist/Foo_Bar.js';\n\
         const a: Dot = dot;\nconst b: Underscore = underscore;\n\
         const both: readonly [Dot, Underscore] = pair;\n\
         // @ts-expect-error these module-owned types are distinct\n\
         const wrong: Dot = underscore;\nexport { a, b, both, wrong };\n",
    )
    .expect("write");
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
    assert!(
        tsc.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&tsc.stdout),
        String::from_utf8_lossy(&tsc.stderr),
    );
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn a_barrel_re_exports_what_it_exposes_from_its_modules() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dts_barrel");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/Lib")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").expect("write");
    fs::write(
        dir.join("src/Lib/Pair.fl"),
        "module Lib.Pair exposing (pair, Box(..), Point)\n\
         type Box = Box Float\n\
         type alias Point = { x : Float; y : Float }\n\
         pair : a -> b -> (a, b)\nlet pair x y = (x, y)\n",
    )
    .expect("write");
    fs::write(
        dir.join("src/Lib.fl"),
        "module Lib exposing (pair, Box(..), Point)\n\
         import Lib.Pair exposing (pair, Box(..), Point)\n",
    )
    .expect("write");
    let build = flick_in(&dir, &["build", "--emit-dts"]);
    assert!(build.status.success(), "{build:?}");
    fs::write(
        dir.join("consumer.ts"),
        "import { pair, Box } from './dist/Lib.js';\n\
         import type { Point } from './dist/Lib.js';\n\
         import type { Box as Own } from './dist/Lib/Pair.js';\n\
         const both: number = pair(1)('x')[0];\n\
         const second: string = pair(1)('x')[1];\n\
         // @ts-expect-error the second is a string, not a number\n\
         const wrong: number = pair(1)('x')[1];\n\
         const box: Own = Box(2);\nconst p: Point = { x: 1, y: 2 };\n\
         export { both, second, wrong, box, p };\n",
    )
    .expect("write");
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
    assert!(
        tsc.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&tsc.stdout),
        String::from_utf8_lossy(&tsc.stderr),
    );
    // the barrel's values are the defining module's, not copies
    let node = Command::new("node")
        .arg("--input-type=module")
        .arg("-e")
        .arg(
            "import * as barrel from './dist/Lib.js';\n\
             import * as own from './dist/Lib/Pair.js';\n\
             console.log(barrel.pair === own.pair, barrel.Box === own.Box, barrel.pair(1)('x'));",
        )
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(
        String::from_utf8_lossy(&node.stdout),
        "true true [ 1, 'x' ]\n",
        "{node:?}"
    );
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn streams_services_and_sync_exports_cross_into_effect_typescript() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("dts_effect_boundary");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src/Lib")).expect("dir");
    fs::write(
        dir.join("flick.toml"),
        "schema-version = 1\n\n[project]\nkind = \"lib\"\n",
    )
    .expect("write");
    fs::write(dir.join("package.json"), "{\"type\":\"module\"}\n").expect("write");
    fs::write(
        dir.join("src/Lib/Bridge.fl"),
        "module Lib.Bridge exposing (numbers, Greeter, greet, count, doubled)\n\n\
         numbers : Stream.Stream none none Int\nlet numbers = Stream.range 1 3\n\n\
         service Greeter =\n  hello : String ->{} String\n\n\
         greet : String ->{Greeter;} String\nlet greet name = Greeter.hello name\n\n\
         count : Atom.Writable Float\nlet count = Atom.make 21\n\n\
         doubled : Unit ->{Atom.Read;} Float\nlet doubled () = Atom.value count * 2\n",
    )
    .expect("write");
    let build = flick_in(&dir, &["build", "--emit-dts"]);
    assert!(build.status.success(), "{build:?}");
    // a TypeScript caller uses Effect's own API on Flick's values
    fs::write(
        dir.join("consumer.ts"),
        "import { Effect } from '@flick-lang/runtime';\n\
         import * as Stream from '@flick-lang/runtime/effect/Stream';\n\
         import { $s_Greeter, doubled, greet, numbers } from './dist/Lib/Bridge.js';\n\
         const collected: Effect.Effect<ReadonlyArray<number>> = Stream.runCollect(numbers);\n\
         const greeted: Effect.Effect<string> = Effect.provideService(greet('ada'), $s_Greeter, {\n\
           hello: (name: string) => Effect.succeed(`hi ${name}`),\n\
         });\n\
         // @ts-expect-error the greeting still needs its Greeter\n\
         const unprovided: Effect.Effect<string> = greet('ada');\n\
         // a Sync export is an Effect that reads the atoms\n\
         const read = doubled(undefined);\n\
         // @ts-expect-error it is not a number\n\
         const wrong: number = doubled(undefined);\n\
         console.log(JSON.stringify(await Effect.runPromise(collected)), await Effect.runPromise(greeted), Effect.isEffect(read));\n\
         export { unprovided, wrong };\n",
    )
    .expect("write");
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
    assert!(
        tsc.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&tsc.stdout),
        String::from_utf8_lossy(&tsc.stderr),
    );
    let node = Command::new("node")
        .arg("consumer.ts")
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(
        String::from_utf8_lossy(&node.stdout),
        "[1,2,3] hi ada true\n",
        "{node:?}"
    );
}
