//! `flick schema export` refuses a codec or an `HttpApi` without a strict
//! wire contract, naming the node, and writes nothing; the same codecs still
//! build. Needs Node and `pnpm install`: run with `--ignored` (part of
//! `just e2e`).

mod common;

use std::fs;

use common::{flick_in, project};

const LIB: &str = "schema-version = 1\n\n[project]\nkind = \"lib\"\n";

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
        &[("flick.toml", LIB), ("src/Lib/Wire.fl", WIRE)],
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
        stderr.contains("has no strict OpenAPI document")
            && stderr.contains("files.upload.payload.data -> JSON Schema"),
        "{stderr}"
    );
    assert!(!dir.join("openapi.json").exists());

    // the refusal belongs to the exporter: the same codecs still build
    let build = flick_in(&dir, &["build"]);
    assert_eq!(build.status.code(), Some(0), "{build:?}");
}
