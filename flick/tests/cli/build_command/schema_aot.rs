use std::fs;
use std::path::Path;
use std::process::Command;

use crate::common::{flick_in, project};
use flick_core::build::runner_path;

const AOT_APP: &str = r#"module Main exposing (main)

type alias Point = { x : Float; y : Float }
  deriving Schema

point_json : PureCodec Json Point
let point_json = Codec.json Point.schema

main : Unit ->{Host.Console; AppError} Unit
let main () =
  match Json.parse "{\"x\": 1, \"y\": 2}" with
  | Err _ -> Platform.stdout_line "not JSON"
  | Ok input ->
    match Codec.decode_result point_json input with
    | Ok p -> Platform.stdout_line (show (p.x + p.y))
    | Err _ -> Platform.stdout_line "not a point"
"#;

#[test]
#[ignore = "needs Node and pnpm install"]
fn schema_aot_writes_compiled_decoders_the_runner_installs() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("build_schema_aot");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("dir");
    fs::write(dir.join("Main.fl"), AOT_APP).expect("write");
    let denied = flick_in(
        &dir,
        &[
            "build",
            "Main.fl",
            "--out-dir",
            "out",
            "--runtime",
            "effect",
            "--schema-aot",
        ],
    );
    assert_eq!(denied.status.code(), Some(1), "{denied:?}");
    assert!(
        String::from_utf8_lossy(&denied.stderr).contains("FLICK-P051"),
        "{denied:?}"
    );
    let out = flick_in(
        &dir,
        &[
            "--allow-tool",
            "node",
            "build",
            "Main.fl",
            "--out-dir",
            "out",
            "--runtime",
            "effect",
            "--schema-aot",
        ],
    );
    assert!(out.status.success(), "{out:?}");
    let aot = fs::read_to_string(dir.join("out/schemas.aot.js")).expect("compiled decoders");
    assert!(aot.contains("from \"@flick-lang/effect-ts\""), "{aot}");
    let run = Command::new("node")
        .arg(Path::new("out").join(runner_path(false)))
        .current_dir(&dir)
        .output()
        .expect("node");
    assert!(run.status.success(), "{run:?}");
    assert_eq!(String::from_utf8_lossy(&run.stdout), "3\n");
    // a stale module is refused, and decoding still works
    fs::write(
        dir.join("out/schemas.aot.js"),
        "export const fingerprint = \"\";\nexport const install = () => {};\n",
    )
    .expect("write");
    let stale = Command::new("node")
        .arg(Path::new("out").join(runner_path(false)))
        .current_dir(&dir)
        .output()
        .expect("node");
    assert_eq!(String::from_utf8_lossy(&stale.stdout), "3\n");
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("stale"),
        "{stale:?}"
    );
    let plain = flick_in(&dir, &["build", "Main.fl", "--out-dir", "plain"]);
    assert!(plain.status.success(), "{plain:?}");
    assert!(!dir.join("plain/schemas.aot.js").exists());

    // Inside a project with `[tools]`, schema-aot outputs are cached across builds.
    let proj = project(
        "build_schema_aot_cached",
        &[
            (
                "flick.toml",
                concat!(
                    "schema-version = 1\n",
                    "[project]\nkind = \"app\"\nentry = \"App.Main\"\n",
                    "[target.js]\nruntime = \"effect\"\nschema-aot = true\n",
                    "[tools]\n\"flick/js\" = [\"node\"]\n",
                ),
            ),
            (
                "src/App/Main.fl",
                &AOT_APP.replace("module Main", "module App.Main"),
            ),
        ],
    );
    let first = flick_in(&proj, &["build"]);
    assert!(first.status.success(), "{first:?}");
    assert!(proj.join("dist/schemas.aot.js").is_file());
    let second = flick_in(&proj, &["build"]);
    assert!(second.status.success(), "{second:?}");
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("Restoring cached artifacts"),
        "{second:?}"
    );
}
