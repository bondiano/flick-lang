//! End-to-end WASM plugin tests for `flick:plugin@0.5.0`:
//! - Configurable-field `Meta.Codec` derive (JS & Go executable, exact field-span
//!   diagnostics on unsupported types, refusal to leak opaque/private types).
//! - CLI/form `Meta.CliForm` derive (`///` docs on types/variants/record fields,
//!   declaration order via `source_index`, variant `@alias` attributes,
//!   executable help and argument parsing).
//! - External plugin-declared `heads` (`declarations`, `named`, `bare`) and
//!   `watch` (with NO `watch` in `flick.toml`).
//! - Derive cache hit vs invalidation on doc/attribute/visibility/type changes.
//! - Incompatible `0.4.0` component rebuild diagnostic (`FLICK-P035`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::{APP, LIB, MAIN, PAIRS, component as pairs_component, flick, trace};
use crate::common::project;

const META_APP: &str = r#"schema-version = 1

[project]
kind = "app"
entry = "App.Main"

[dependencies]
meta = { path = "../meta" }
"#;

const META_APP_GO: &str = r#"schema-version = 1

[project]
kind = "app"
entry = "App.Main"

[dependencies]
meta = { path = "../meta" }

[build]
target = "go"

[tools]
"flick/go" = ["go"]
"#;

/// Notice: NO `watch = [...]` in `flick.toml` — `Meta.guarded` comes purely from
/// `metadata().watch`!
const META_LIB_TOML: &str = r#"schema-version = 1

[package]
name = "meta"
version = "1.0.0"
plugin = "meta.wasm"

[project]
kind = "lib"
"#;

const META_MODULE: &str = r#"module Meta exposing (..)

@derive
class Codec a =
  encode : a -> String
  decode : String -> Result String a

@derive
class CliForm a =
  help_text : Option a -> String
  parse_form : List String -> Result String a

guarded : String -> String
let guarded s = s

join_kv : List String -> String
let join_kv parts = String.join ";" parts

encode_int : Int -> String
let encode_int n = show n

encode_bool : Bool -> String
let encode_bool b = if b then "true" else "false"

encode_opt_string : Option String -> String
let encode_opt_string opt =
  match opt with
    | Some s -> s
    | None -> ""

strip_tag : String -> String -> Result String String
let strip_tag tag raw =
  let prefix = tag ++ ":"
  if String.starts_with prefix raw then
    Ok (String.slice (String.length prefix) (String.length raw) raw)
  else
    Err ("expected tag " ++ tag)

kv_get : String -> String -> Option String
let kv_get key raw =
  let prefix = key ++ "="
  let parts = String.split ";" raw
  match List.find (fun p -> String.starts_with prefix p) parts with
    | Some entry -> Some (String.slice (String.length prefix) (String.length entry) entry)
    | None -> None

decode_string : String -> String -> Result String String
let decode_string key raw =
  match kv_get key raw with
    | Some v -> Ok v
    | None -> Err ("missing " ++ key)

decode_string_default : String -> String -> String -> Result String String
let decode_string_default key fallback raw =
  match kv_get key raw with
    | Some v -> Ok v
    | None -> Ok fallback

decode_opt_string : String -> String -> Result String (Option String)
let decode_opt_string key raw =
  match kv_get key raw with
    | Some "" -> Ok None
    | Some v -> Ok (Some v)
    | None -> Ok None

decode_int : String -> String -> Result String Int
let decode_int key raw =
  match kv_get key raw with
    | Some v ->
      match Int.parse v with
        | Some n -> Ok n
        | None -> Err ("bad int for " ++ key)
    | None -> Err ("missing " ++ key)

decode_int_default : String -> Int -> String -> Result String Int
let decode_int_default key fallback raw =
  match kv_get key raw with
    | Some v ->
      match Int.parse v with
        | Some n -> Ok n
        | None -> Err ("bad int for " ++ key)
    | None -> Ok fallback

decode_bool : String -> String -> Result String Bool
let decode_bool key raw =
  match kv_get key raw with
    | Some "true" -> Ok true
    | Some "false" -> Ok false
    | Some _ -> Err ("bad bool for " ++ key)
    | None -> Err ("missing " ++ key)

decode_bool_default : String -> Bool -> String -> Result String Bool
let decode_bool_default key fallback raw =
  match kv_get key raw with
    | Some "true" -> Ok true
    | Some "false" -> Ok false
    | Some _ -> Err ("bad bool for " ++ key)
    | None -> Ok fallback

bind_res : Result String a -> (a -> Result String b) -> Result String b
let bind_res r f =
  match r with
    | Ok v -> f v
    | Err e -> Err e

flag_lookup : String -> List String -> Option String
let rec flag_lookup flag args =
  match List.head args with
    | Some a ->
      let rest = match List.tail args with | Some t -> t | None -> []
      if a == flag then List.head rest else flag_lookup flag rest
    | None -> None

flag_string : String -> List String -> Result String String
let flag_string flag args =
  match flag_lookup flag args with
    | Some v -> Ok v
    | None -> Err ("missing " ++ flag)

flag_opt_string : String -> List String -> Result String (Option String)
let flag_opt_string flag args = Ok (flag_lookup flag args)

flag_int : String -> List String -> Result String Int
let flag_int flag args =
  match flag_lookup flag args with
    | Some v ->
      match Int.parse v with
        | Some n -> Ok n
        | None -> Err ("bad int for " ++ flag)
    | None -> Err ("missing " ++ flag)

flag_bool : String -> List String -> Result String Bool
let flag_bool flag args =
  match flag_lookup flag args with
    | Some "true" -> Ok true
    | Some "false" -> Ok false
    | Some _ -> Err ("bad bool for " ++ flag)
    | None -> Ok false

cmd_head : List String -> Option (String, List String)
let cmd_head args =
  match List.head args with
    | Some h ->
      let rest = match List.tail args with | Some t -> t | None -> []
      Some (h, rest)
    | None -> None

matches_cmd : String -> List String -> Bool
let matches_cmd cmd names = List.any (fun n -> n == cmd) names
"#;

fn meta_component() -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../target/wasm32-wasip2/release/flick_test_meta.wasm");
    assert!(
        path.exists(),
        "no {}: run `just wasm-plugins`",
        path.display()
    );
    path
}

fn meta_app(name: &str, app_toml: &str, extra_files: &[(&str, &str)]) -> PathBuf {
    let mut files = vec![
        ("app/flick.toml", app_toml),
        ("meta/flick.toml", META_LIB_TOML),
        ("meta/src/Meta.fl", META_MODULE),
    ];
    files.extend_from_slice(extra_files);
    let root = project(name, &files);
    fs::copy(meta_component(), root.join("meta/meta.wasm")).expect("copy meta component");
    root
}

fn run_flick(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_flick"))
        .args(args)
        .current_dir(root.join("app"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("FLICK_PLUGIN_TRACE", root.join("trace"))
        .env("NO_COLOR", "1")
        .env_remove("CI")
        .output()
        .expect("flick runs")
}

#[test]
fn external_codec_derive_executes_on_js_and_go_with_rename_skip_and_default() {
    let main = r#"module App.Main exposing (main, User(..))

import Meta

/// Account payload
@label usr
type User = User {
  /// Wire key renamed to user_id
  userName : String;
  /// Skipped on wire, defaulted on decode
  internalTag : String;
  /// Defaulted when absent on wire
  retries : Int;
  active : Bool
} deriving Meta.Codec {
  rename userName "user_id";
  skip internalTag;
  default internalTag "local";
  default retries 3
}

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let u = User { userName = "ada"; internalTag = "secret"; retries = 5; active = true }
  let wire = Meta.encode u
  let decoded = Meta.decode "usr:user_id=grace;active=false"
  let summary =
    match decoded with
      | Ok (User r) -> r.userName ++ "|" ++ r.internalTag ++ "|" ++ show r.retries ++ "|" ++ Meta.encode_bool r.active
      | Err e -> "ERR:" ++ e
  Platform.stdout_line (wire ++ "\n" ++ summary)
"#;
    let expected = "usr:user_id=ada;retries=5;active=true\ngrace|local|3|false\n";

    // JS execution
    let js_root = meta_app("meta-codec-js", META_APP, &[("app/src/App/Main.fl", main)]);
    let js_out = run_flick(&js_root, &["run"]);
    assert!(
        js_out.status.success(),
        "js failed: {}",
        String::from_utf8_lossy(&js_out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&js_out.stdout), expected);

    // Go execution
    let go_root = meta_app(
        "meta-codec-go",
        META_APP_GO,
        &[("app/src/App/Main.fl", main)],
    );
    let go_out = run_flick(&go_root, &["run"]);
    assert!(
        go_out.status.success(),
        "go failed: {}",
        String::from_utf8_lossy(&go_out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&go_out.stdout), expected);
}

#[test]
fn external_codec_rejects_unsupported_field_at_exact_span_and_refuses_opaque_or_private_leak() {
    // 1. Unsupported field type points at the exact field type span (`Int -> Int`).
    let unsupported = r"module App.Main exposing (main, Bad(..))

import Meta

type Bad = Bad {
  ok : String;
  broken : Int -> Int
} deriving Meta.Codec

main : Unit ->{; AppError} Unit
let main () = ()
";
    let root = meta_app(
        "meta-codec-bad-field",
        META_APP,
        &[("app/src/App/Main.fl", unsupported)],
    );
    let out = run_flick(&root, &["check", "--diagnostics", "json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("meta/M001"), "{stderr}");
    // Verify the exact field type span (`Int -> Int` on line 7, bytes matching `Int -> Int`).
    let start = unsupported.find("Int -> Int").expect("field type offset");
    let end = start + "Int -> Int".len();
    assert!(
        stderr.contains(&format!("\"start\":{start}"))
            && stderr.contains(&format!("\"end\":{end}")),
        "diagnostic did not point at exact field type span {start}..{end}:\n{stderr}"
    );

    // 2. Public record referencing an opaque type refuses to leak it at the field span.
    let leak_opaque = r"module App.Main exposing (main, Secret, PublicDto(..))

import Meta

opaque type Secret = Secret String

type PublicDto = PublicDto {
  id : String;
  token : Secret
} deriving Meta.Codec

main : Unit ->{; AppError} Unit
let main () = ()
";
    let root_opaque = meta_app(
        "meta-codec-opaque-leak",
        META_APP,
        &[("app/src/App/Main.fl", leak_opaque)],
    );
    let out_opaque = run_flick(&root_opaque, &["check", "--diagnostics", "json"]);
    let stderr_opaque = String::from_utf8_lossy(&out_opaque.stderr);
    assert!(!out_opaque.status.success(), "{stderr_opaque}");
    assert!(stderr_opaque.contains("meta/M002"), "{stderr_opaque}");
    let field_start = leak_opaque.find("token : Secret").expect("field offset");
    let field_end = field_start + "token : Secret".len();
    assert!(
        stderr_opaque.contains(&format!("\"start\":{field_start}"))
            && stderr_opaque.contains(&format!("\"end\":{field_end}")),
        "M002 did not point at exact field span {field_start}..{field_end}:\n{stderr_opaque}"
    );

    // 3. `deriving instance` on a foreign opaque type also refuses with M002.
    let foreign_secrets =
        "module App.Secrets exposing (Token)\n\nopaque type Token = Token { raw : String }\n";
    let main_foreign = r"module App.Main exposing (main)

import Meta
import App.Secrets

deriving instance Meta.Codec (App.Secrets.Token)

main : Unit ->{; AppError} Unit
let main () = ()
";
    let root_foreign = meta_app(
        "meta-codec-foreign-opaque",
        META_APP,
        &[
            ("app/src/App/Secrets.fl", foreign_secrets),
            ("app/src/App/Main.fl", main_foreign),
        ],
    );
    let out_foreign = run_flick(&root_foreign, &["check"]);
    let stderr_foreign = String::from_utf8_lossy(&out_foreign.stderr);
    assert!(!out_foreign.status.success(), "{stderr_foreign}");
    assert!(stderr_foreign.contains("meta/M002"), "{stderr_foreign}");
}

#[test]
fn external_cli_form_derive_uses_docs_attributes_and_source_index_order() {
    let main = r#"module App.Main exposing (main, Form(..), Command(..))

import Meta

/// Deployment form options
@label form
type Form = Form {
  /// Target environment name
  zebra : String;
  /// Worker replica count
  alpha : Int;
  /// Optional release note
  middle : Option String
} deriving Meta.CliForm

/// CLI subcommands
type Command =
  /// Start the HTTP server
  Serve {
    /// Port to bind
    port : Int;
    /// Host interface
    host : String
  }
  /// Show current status
  | Status
  deriving Meta.CliForm { alias Serve "s" }

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let form_help = Meta.help_text (None : Option Form)
  let cmd_help = Meta.help_text (None : Option Command)
  let parsed_form = Meta.parse_form ["--zebra", "prod", "--alpha", "4", "--middle", "v1"]
  let form_line =
    match parsed_form with
      | Ok (Form f) ->
        let note = Option.with_default "none" f.middle
        f.zebra ++ ":" ++ show f.alpha ++ ":" ++ note
      | Err e -> "ERR:" ++ e
  let parsed_cmd = Meta.parse_form ["s", "--port", "8080", "--host", "127.0.0.1"]
  let cmd_line =
    match parsed_cmd with
      | Ok (Serve cfg) -> "serve:" ++ cfg.host ++ ":" ++ show cfg.port
      | Ok Status -> "status"
      | Err e -> "ERR:" ++ e
  Platform.stdout_line (form_help ++ "\n---\n" ++ cmd_help ++ "\n---\n" ++ form_line ++ "\n" ++ cmd_line)
"#;
    let root = meta_app("meta-cli-form", META_APP, &[("app/src/App/Main.fl", main)]);
    let out = run_flick(&root, &["run"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let expected = "\
Form: Deployment form options
  --zebra <String> - Target environment name
  --alpha <Int> - Worker replica count
  --middle <Option String> - Optional release note
---
Command: CLI subcommands
  serve (alias: s): Start the HTTP server
    --port <Int> - Port to bind
    --host <String> - Host interface
  status: Show current status
---
prod:4:v1
serve:127.0.0.1:8080
";
    assert_eq!(String::from_utf8_lossy(&out.stdout), expected);
}

#[test]
fn external_wasm_declared_heads_watch_and_derive_cache_invalidation() {
    let main = r#"module App.Main exposing (main, Item(..))

import Meta exposing (plugin route, plugin widget, plugin ref_of)

route fn ping = "pong"

widget title = "hello"

/// Item doc
@label it
type Item = Item {
  /// Item label
  label : String
} deriving Meta.Codec, Meta.CliForm

main : Unit ->{Host.Console; AppError} Unit
let main () =
  let ref_name = ref_of App.Main.title
  let ok = Meta.guarded "allowed"
  let wire = Meta.encode (Item { label = "box" })
  Platform.stdout_line (ping ++ "|" ++ title ++ "|" ++ ref_name ++ "|" ++ ok ++ "|" ++ wire)
"#;
    let root = meta_app(
        "meta-heads-watch",
        META_APP,
        &[("app/src/App/Main.fl", main)],
    );

    // First run executes external heads (`route fn`, `widget`, `ref_of`) and derives.
    let ran = run_flick(&root, &["run"]);
    assert!(
        ran.status.success(),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout),
        "pong|hello|App.Main.title|allowed|it:label=box\n"
    );
    let initial_derives = trace(&root)
        .iter()
        .filter(|l| l.starts_with("derive meta "))
        .count();
    assert_eq!(initial_derives, 2, "{:?}", trace(&root));

    // Second `flick check` hits derive cache (0 new derives).
    let checked = run_flick(&root, &["check"]);
    assert!(checked.status.success(), "{checked:?}");
    let after_hit = trace(&root)
        .iter()
        .filter(|l| l.starts_with("derive meta "))
        .count();
    assert_eq!(after_hit, 2, "{:?}", trace(&root));

    // Editing a `///` doc comment invalidates the WASM derive cache!
    let edited_doc = main.replace("Item doc", "Updated item doc");
    fs::write(root.join("app/src/App/Main.fl"), &edited_doc).expect("write");
    let checked2 = run_flick(&root, &["check"]);
    assert!(checked2.status.success(), "{checked2:?}");
    let after_doc_edit = trace(&root)
        .iter()
        .filter(|l| l.starts_with("derive meta "))
        .count();
    assert_eq!(after_doc_edit, 4, "{:?}", trace(&root));

    // Metadata-declared `watch = ["Meta.guarded"]` (with NO `watch` in flick.toml)
    // catches forbidden calls!
    let forbidden = edited_doc.replace("\"allowed\"", "\"forbidden\"");
    fs::write(root.join("app/src/App/Main.fl"), &forbidden).expect("write");
    let watched = run_flick(&root, &["check"]);
    let stderr = String::from_utf8_lossy(&watched.stderr);
    assert!(!watched.status.success(), "{stderr}");
    assert!(stderr.contains("meta/M003"), "{stderr}");
}

#[test]
fn incompatible_0_4_0_wasm_component_reports_p035_rebuild_diagnostic() {
    let root = project(
        "plugins-incompatible-0-4-0",
        &[
            ("app/flick.toml", APP),
            ("app/src/App/Main.fl", MAIN),
            ("lib/flick.toml", LIB),
            ("lib/src/Pairs.fl", PAIRS),
        ],
    );
    let mut bytes = fs::read(pairs_component()).expect("read pairs component");
    let needle = b"flick:plugin/compiler@0.5.0";
    let pos = bytes
        .windows(needle.len())
        .position(|w| w == needle)
        .expect("component imports flick:plugin/compiler@0.5.0");
    bytes[pos..pos + needle.len()].copy_from_slice(b"flick:plugin/compiler@0.4.0");
    fs::write(root.join("lib/pairs.wasm"), bytes).expect("write patched component");

    let out = flick(&root, &["check", "--diagnostics", "json"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{stderr}");
    assert!(stderr.contains("FLICK-P035"), "{stderr}");
    assert!(stderr.contains("flick:plugin@0.4.0"), "{stderr}");
    assert!(stderr.contains("0.5.0"), "{stderr}");
    assert!(stderr.contains("rebuild it"), "{stderr}");
}
