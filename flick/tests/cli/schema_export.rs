//! `flick schema export` (contributed by `std`) runs an in-memory Flick
//! exporter program through the project's selected backend (`flick/js` on RT
//! or Effect, or `flick/go` as a native executable without Node). It refuses
//! codecs or `HttpApi` values without a strict wire contract (`FLICK-D008`
//! with a source span into the exported declaration) and writes nothing.

use crate::common;

use std::env;
use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{flick_in, project};

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";
const GO_LIB: &str = "schema-version = 1\n\n[tools]\n\"flick/go\" = [\"go\"]\n\n[project]\nkind = \"lib\"\n\n[build]\ntarget = \"go\"\n";

const RT_WIRE: &str = "module Lib.Wire exposing (Item, item_json)\n\n\
type alias Item = { id : String }\n  deriving Schema\n\n\
item_json : PureCodec Json Item\nlet item_json = Codec.json Item.schema\n";

const PORTABLE_API_WIRE: &str = r#"module Lib.Wire exposing (ItemPath, Item, item_json, get_item, api)

type alias ItemPath = { id : String }
  deriving Schema

type alias Item = { id : String; count : Int }
  deriving Schema

path_json : PureCodec Json ItemPath
let path_json = Codec.json ItemPath.schema

item_json : PureCodec Json Item
let item_json = Codec.json Item.schema

get_item : HttpApi.Endpoint "get_item" (HttpApi.Request ItemPath Unit Unit Unit ->{r;} Item)
let get_item =
  HttpApi.endpoint HttpApi.Get "/items/:id" (Record.label : Record.Label "get_item")
    |> HttpApi.with_path path_json
    |> HttpApi.with_success 200 item_json

api : HttpApi.Api r { items : { get_item : HttpApi.Request ItemPath Unit Unit Unit ->{r;} Item } }
let api =
  HttpApi.api "Items"
    |> HttpApi.add_group (HttpApi.group (Record.label : Record.Label "items") |> HttpApi.add get_item)
"#;

fn find_on_path(name: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

/// Builds a `PATH` containing `go` and no `node` executable so Go tests prove
/// zero dependency on Node.
fn path_without_node(dir: &Path) -> OsString {
    let go = find_on_path("go").expect("`go` must be on PATH");
    let bin = dir.join(".no-node-bin");
    let _ = fs::remove_dir_all(&bin);
    fs::create_dir_all(&bin).expect("bin dir");
    let link = bin.join(go.file_name().expect("go filename"));
    #[cfg(unix)]
    symlink(&go, &link).expect("symlink go");
    #[cfg(windows)]
    fs::copy(&go, &link).expect("copy go");
    assert!(
        Command::new("node")
            .arg("--version")
            .env("PATH", &bin)
            .output()
            .is_err(),
        "node must not be reachable in node-free PATH"
    );
    bin.into_os_string()
}

fn flick_in_without_node(dir: &Path, args: &[&str]) -> Output {
    let path = path_without_node(dir);
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(dir)
        .env("PATH", path)
        .env_remove("NO_COLOR")
        .output()
        .expect("flick runs")
}

#[test]
fn schema_export_rejects_malformed_codec_and_unknown_target_with_p018() {
    let dir = project(
        "schema_export_malformed",
        &[("flick.toml", LIB), ("src/Lib/Wire.fl", RT_WIRE)],
    );
    let bad_codec = flick_in(
        &dir,
        &[
            "schema",
            "export",
            "not_qualified",
            "--target",
            "json-schema-2020-12",
            "--out",
            "item.json",
        ],
    );
    assert_eq!(bad_codec.status.code(), Some(2), "{bad_codec:?}");
    let stderr = String::from_utf8_lossy(&bad_codec.stderr);
    assert!(stderr.contains("FLICK-P018"), "{stderr}");
    assert!(!dir.join("item.json").exists());

    let bad_target = flick_in(
        &dir,
        &[
            "std:schema",
            "export",
            "Lib.Wire.item_json",
            "--target",
            "yaml-schema",
            "--out",
            "item.json",
        ],
    );
    assert_eq!(bad_target.status.code(), Some(2), "{bad_target:?}");
    let stderr = String::from_utf8_lossy(&bad_target.stderr);
    assert!(stderr.contains("FLICK-P018"), "{stderr}");
    assert!(!dir.join("item.json").exists());
}

#[test]
fn schema_export_reports_type_error_anchored_to_original_source() {
    let wire =
        "module Lib.Wire exposing (not_a_codec)\n\nnot_a_codec : Int\nlet not_a_codec = 42\n";
    let dir = project(
        "schema_export_type_error_span",
        &[("flick.toml", LIB), ("src/Lib/Wire.fl", wire)],
    );
    let out = flick_in(
        &dir,
        &[
            "schema",
            "export",
            "Lib.Wire.not_a_codec",
            "--target",
            "json-schema-2020-12",
            "--out",
            "out.json",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FLICK-T003"), "{stderr}");
    assert!(
        stderr.contains("src/Lib/Wire.fl:4:"),
        "expected source span at `let not_a_codec` in src/Lib/Wire.fl:\n{stderr}"
    );
    assert!(!dir.join("out.json").exists());
    assert!(!dir.join("src/FlickSchemaExport.fl").exists());
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn rt_schema_export_uses_the_native_codec() {
    let dir = project(
        "schema_export_rt",
        &[("flick.toml", LIB), ("src/Lib/Wire.fl", RT_WIRE)],
    );
    let out = flick_in(
        &dir,
        &[
            "schema",
            "export",
            "Lib.Wire.item_json",
            "--target",
            "json-schema-2020-12",
            "--out",
            "item.json",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let item = fs::read_to_string(dir.join("item.json")).expect("schema");
    assert!(
        item.contains("\"id\"") && item.contains("2020-12"),
        "{item}"
    );
    // Qualified `std:schema export` yields identical output and leaves no overlay files.
    let qualified = flick_in(
        &dir,
        &[
            "std:schema",
            "export",
            "Lib.Wire.item_json",
            "--target",
            "json-schema-2020-12",
            "--out",
            "item-qualified.json",
        ],
    );
    assert_eq!(qualified.status.code(), Some(0), "{qualified:?}");
    assert_eq!(
        fs::read(dir.join("item-qualified.json")).expect("qualified schema"),
        item.as_bytes()
    );
    assert!(!dir.join("src/FlickSchemaExport.fl").exists());
}

#[test]
#[ignore = "needs Node and pnpm install"]
fn schema_export_succeeds_on_typescript_and_declarations_projects() {
    let config = format!(
        "{LIB}\n[target.js]\ntypescript = true\ndeclarations = true\npromise-facade = true\nschema-aot = true\n"
    );
    let dir = project(
        "schema_export_typescript_project",
        &[("flick.toml", &config), ("src/Lib/Wire.fl", RT_WIRE)],
    );
    let out = flick_in(
        &dir,
        &[
            "schema",
            "export",
            "Lib.Wire.item_json",
            "--target",
            "json-schema-2020-12",
            "--out",
            "item.json",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let item = fs::read_to_string(dir.join("item.json")).expect("schema");
    assert!(
        item.contains("\"id\"") && item.contains("2020-12"),
        "{item}"
    );
}

#[test]
#[ignore = "needs Go"]
fn schema_export_succeeds_on_go_target_project() {
    let go_wire = "module Lib.Wire exposing (Item, item_json, get_item, api, checked_id)

effect Ask =
  control ask : Int ->{} Int

checked_id : Unit ->{} Int
let checked_id () =
  try Ask.ask 1 with
  | Ask.ask x -> resume (x + 1)

type alias Item = { id : String }
  deriving Schema

item_json : PureCodec Json Item
let item_json = Codec.json Item.schema

get_item : HttpApi.Endpoint \"get_item\" (HttpApi.Request Item Unit Unit Unit ->{r;} Item)
let get_item =
  HttpApi.endpoint HttpApi.Get \"/items/:id\" (Record.label : Record.Label \"get_item\")
    |> HttpApi.with_path item_json
    |> HttpApi.with_success 200 item_json

api : HttpApi.Api r { items : { get_item : HttpApi.Request Item Unit Unit Unit ->{r;} Item } }
let api =
  HttpApi.api \"Items\"
    |> HttpApi.add_group (HttpApi.group (Record.label : Record.Label \"items\") |> HttpApi.add get_item)
";
    let dir = project(
        "schema_export_go_project",
        &[("flick.toml", GO_LIB), ("src/Lib/Wire.fl", go_wire)],
    );
    let out = flick_in_without_node(
        &dir,
        &[
            "schema",
            "export",
            "Lib.Wire.item_json",
            "--target",
            "json-schema-2020-12",
            "--out",
            "item.json",
        ],
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let item = fs::read_to_string(dir.join("item.json")).expect("schema");
    assert!(
        item.contains("\"id\"") && item.contains("2020-12"),
        "{item}"
    );

    let openapi = flick_in_without_node(
        &dir,
        &[
            "std:schema",
            "export",
            "Lib.Wire.api",
            "--target",
            "openapi-3.1",
            "--out",
            "openapi.json",
        ],
    );
    assert_eq!(openapi.status.code(), Some(0), "{openapi:?}");
    let doc = fs::read_to_string(dir.join("openapi.json")).expect("openapi");
    assert!(
        doc.contains("\"openapi\"") && doc.contains("\"3.1") && doc.contains("/items/{id}"),
        "{doc}"
    );
    assert!(!dir.join("src/FlickSchemaExport.fl").exists());
}

fn export_bytes(dir: &Path, args: &[&str], out_file: &str, without_node: bool) -> Vec<u8> {
    let out = if without_node {
        flick_in_without_node(dir, args)
    } else {
        flick_in(dir, args)
    };
    assert_eq!(out.status.code(), Some(0), "{args:?}: {out:?}");
    fs::read(dir.join(out_file)).expect("exported output")
}

#[test]
#[ignore = "needs Go and Node"]
fn schema_export_is_byte_identical_across_js_rt_js_effect_and_go() {
    let effect_lib = format!("{LIB}\n[target.js]\nruntime = \"effect\"\n");
    let item_wire = "module Lib.Wire exposing (Item, item_json)\n\n\
type alias Item = { id : String; count : Int }\n  deriving Schema\n\n\
item_json : PureCodec Json Item\nlet item_json = Codec.json Item.schema\n";

    // 1. JSON Schema 2020-12: byte-identical across JS RT, JS Effect, and Go (without Node).
    let rt_dir = project(
        "schema_export_parity_rt",
        &[("flick.toml", LIB), ("src/Lib/Wire.fl", item_wire)],
    );
    let effect_dir = project(
        "schema_export_parity_effect",
        &[("flick.toml", &effect_lib), ("src/Lib/Wire.fl", item_wire)],
    );
    let go_dir = project(
        "schema_export_parity_go",
        &[("flick.toml", GO_LIB), ("src/Lib/Wire.fl", item_wire)],
    );
    let json_args = [
        "schema",
        "export",
        "Lib.Wire.item_json",
        "--target",
        "json-schema-2020-12",
        "--out",
        "item.json",
    ];
    let rt_bytes = export_bytes(&rt_dir, &json_args, "item.json", false);
    let effect_bytes = export_bytes(&effect_dir, &json_args, "item.json", false);
    let go_bytes = export_bytes(&go_dir, &json_args, "item.json", true);
    assert_eq!(rt_bytes, effect_bytes, "JS RT vs JS Effect json-schema");
    assert_eq!(rt_bytes, go_bytes, "JS RT vs Go json-schema");

    // 2. OpenAPI 3.1: byte-identical across JS Effect and Go (without Node).
    let effect_api_dir = project(
        "schema_export_parity_api_effect",
        &[
            ("flick.toml", &effect_lib),
            ("src/Lib/Wire.fl", PORTABLE_API_WIRE),
        ],
    );
    let go_api_dir = project(
        "schema_export_parity_api_go",
        &[
            ("flick.toml", GO_LIB),
            ("src/Lib/Wire.fl", PORTABLE_API_WIRE),
        ],
    );
    let api_args = [
        "schema",
        "export",
        "Lib.Wire.api",
        "--target",
        "openapi-3.1",
        "--out",
        "openapi.json",
    ];
    let effect_api = export_bytes(&effect_api_dir, &api_args, "openapi.json", false);
    let go_api = export_bytes(&go_api_dir, &api_args, "openapi.json", true);
    assert_eq!(effect_api, go_api, "JS Effect vs Go openapi-3.1");
}

const WIRE: &str = r#"module Lib.Wire exposing (..)

type alias Upload = { name : String; data : Unknown }
  deriving Schema

type alias Item = { id : String }
  deriving Schema

upload_json : PureCodec Json Upload
let upload_json = Codec.json Upload.schema

item_json : PureCodec Json Item
let item_json = Codec.json Item.schema

upload : HttpApi.Endpoint "upload" (HttpApi.Request Item Unit Unit Upload ->{r;} Unit)
let upload =
  HttpApi.endpoint HttpApi.Post "/files/:id" (Record.label : Record.Label "upload")
    |> HttpApi.with_path item_json
    |> HttpApi.with_payload upload_json

api : HttpApi.Api r { files : { upload : HttpApi.Request Item Unit Unit Upload ->{r;} Unit } }
let api = HttpApi.api "Files" |> HttpApi.add_group (HttpApi.group (Record.label : Record.Label "files") |> HttpApi.add upload)
"#;

#[test]
#[ignore = "needs Node and pnpm install"]
fn strict_export_refuses_what_it_cannot_state() {
    let dir = project(
        "schema_export_strict",
        &[
            (
                "flick.toml",
                &format!("{LIB}\n[target.js]\nruntime = \"effect\"\n"),
            ),
            ("src/Lib/Wire.fl", WIRE),
        ],
    );
    let export = |codec: &str, target: &str, out: &str| {
        flick_in(
            &dir,
            &["schema", "export", codec, "--target", target, "--out", out],
        )
    };

    let ok = export("Lib.Wire.item_json", "json-schema-2020-12", "item.json");
    assert_eq!(ok.status.code(), Some(0), "{ok:?}");
    let item = fs::read_to_string(dir.join("item.json")).expect("schema");
    assert!(!item.contains("{}") && item.contains("\"id\""), "{item}");

    let codec = export("Lib.Wire.upload_json", "json-schema-2020-12", "upload.json");
    assert_eq!(codec.status.code(), Some(1), "{codec:?}");
    let stderr = String::from_utf8_lossy(&codec.stderr);
    assert!(stderr.contains("FLICK-D008"), "{stderr}");
    assert!(
        stderr.contains("src/Lib/Wire.fl:10:"),
        "expected source span at `let upload_json`:\n{stderr}"
    );
    assert!(
        stderr.contains("`Lib.Wire.upload_json` has no strict JSON Schema")
            && stderr.contains("data -> JSON Schema: an Unknown value accepts anything"),
        "{stderr}"
    );
    assert!(!dir.join("upload.json").exists());

    let openapi = export("Lib.Wire.api", "openapi-3.1", "openapi.json");
    assert_eq!(openapi.status.code(), Some(1), "{openapi:?}");
    let stderr = String::from_utf8_lossy(&openapi.stderr);
    assert!(stderr.contains("FLICK-D008"), "{stderr}");
    assert!(
        stderr.contains("src/Lib/Wire.fl:22:"),
        "expected source span at `let api`:\n{stderr}"
    );
    assert!(
        stderr.contains("has no strict OpenAPI document")
            && stderr.contains("files.upload.payload.data -> JSON Schema"),
        "{stderr}"
    );
    assert!(!dir.join("openapi.json").exists());

    // the refusal belongs to the exporter: the same codecs still build
    let build = flick_in(&dir, &["build"]);
    assert_eq!(build.status.code(), Some(0), "{build:?}");
}
