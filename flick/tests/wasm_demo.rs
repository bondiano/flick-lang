mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use common::flick_in;
use flick_core::build::runner_path;
use flick_wasm::Session;
use serde_json::Value;

const MODULE: &str = "module Main exposing (total, main)\n\nimport List\n\ntotal : List Float -> Float\nlet total xs = List.fold_left (fun acc x -> acc + x) 0 xs\n\nmain : Unit ->{none; AppError} Unit\nlet main () =\n  Platform.stdout_line (Float.to_string (total [1, 2, 3]))\n";

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("json")
}

/// A UTF-16 offset of the playground's answers, as an index.
fn unit(offset: &Value) -> usize {
    usize::try_from(offset.as_u64().expect("an offset")).expect("small")
}

/// The playground's session after an edit to `source`.
fn session(source: &str) -> Session {
    let mut session = Session::new();
    session.update(source);
    session
}

#[test]
fn the_demo_compiles_a_module_as_flick_build_does() {
    let session = session(MODULE);
    for profile in ["debug", "release"] {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("wasm_demo_{profile}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join("Main.fl"), MODULE).expect("write");
        let out = flick_in(
            &dir,
            &["build", "Main.fl", "--profile", profile, "--emit-dts"],
        );
        assert!(out.status.success(), "{out:?}");
        let built = fs::read_to_string(dir.join("dist/Main.js")).expect("module");
        let declared = fs::read_to_string(dir.join("dist/Main.d.ts")).expect("declarations");
        let demo = json(&session.emit(profile));
        assert_eq!(demo["diagnostics"], Value::Array(Vec::new()));
        assert_eq!(demo["js"].as_str(), Some(built.as_str()), "{profile}");
        assert_eq!(demo["dts"].as_str(), Some(declared.as_str()), "{profile}");
    }
}

#[test]
fn app_artifacts_do_not_collide_on_case_insensitive_filesystems() {
    let session = session(MODULE);
    for profile in ["debug", "release"] {
        let built = json(&session.emit(profile));
        let mut paths = BTreeSet::new();
        for path in built["files"].as_object().expect("files").keys() {
            assert!(
                paths.insert(path.to_lowercase()),
                "{profile}: case-insensitive artifact collision at `{path}`"
            );
        }
    }
}

#[test]
fn the_demo_reports_diagnostics_as_json_events() {
    let broken = "module Main exposing (x)\nx : Float\nlet x = \"no\"\n";
    let mut session = Session::new();
    let checked = json(&session.update(broken));
    let event = &checked[0];
    assert_eq!(event["type"], "diagnostic");
    assert!(
        event["code"]
            .as_str()
            .is_some_and(|c| c.starts_with("FLICK-T")),
        "{event}"
    );
    assert_eq!(event["at"], "3:9", "{event}");
    let emitted = json(&session.emit("debug"));
    assert_eq!(emitted["js"], Value::Null);
    assert_eq!(emitted["diagnostics"], checked);
    assert_eq!(json(&session.update(MODULE)), Value::Array(Vec::new()));
}

#[test]
fn the_demo_builds_everything_a_run_needs() {
    let built = json(&session(MODULE).emit("debug"));
    assert_eq!(built["entry"], runner_path(false));
    let files = built["files"].as_object().expect("files");
    assert!(files.contains_key("Main.js") && files.contains_key("_flick/Platform.js"));
    assert!(files.contains_key(built["entry"].as_str().expect("runner")));
    let library = "module Lib exposing (one)\none : Float\nlet one = 1\n";
    let built = json(&session(library).emit("release"));
    assert_eq!(built["entry"], Value::Null);
    let broken = "module Main exposing (x)\nx : Float\nlet x = \"no\"\n";
    let built = json(&session(broken).emit("debug"));
    assert_eq!(built["files"], Value::Null);
}

/// An editor counts in UTF-16 units: a Cyrillic letter is one unit and two
/// bytes, an emoji two units and four bytes.
#[test]
fn positions_cross_the_boundary_in_utf16() {
    let text = "// привет 🎉\nx";
    let x = text.find('x').expect("x");
    let x_byte = u32::try_from(x).expect("small");
    let x_unit = u32::try_from(text[..x].encode_utf16().count()).expect("small");
    assert_eq!(x_unit, 13);
    assert_eq!(flick_wasm::utf16_offset(text, x_byte), x_unit);
    assert_eq!(flick_wasm::byte_offset(text, x_unit), x_byte);
    // inside the emoji's surrogate pair: the start of the emoji
    let emoji = u32::try_from(text.find('🎉').expect("emoji")).expect("small");
    assert_eq!(flick_wasm::byte_offset(text, 11), emoji);
    let source = "module Main exposing (x)\n// ёлка 🎄\nx : Float\nlet x = \"no\"\n";
    let event = &json(&Session::new().update(source))[0];
    let units: Vec<u16> = source.encode_utf16().collect();
    assert_eq!(
        String::from_utf16_lossy(&units[unit(&event["from"])..unit(&event["to"])]),
        "\"no\"",
        "{event}"
    );
    assert_eq!(event["at"], "4:9", "{event}");
}

#[test]
fn hover_and_completion_answer_at_utf16_offsets() {
    let source = "module Main exposing (main)\n// ёж 🦔\nmain : Unit ->{} Unit\nlet main () = ()\n";
    let session = session(source);
    let inside_main = source.rfind("main").expect("main") + 1;
    let at = u32::try_from(source[..inside_main].encode_utf16().count()).expect("small");
    let hover = json(&session.hover(at));
    assert!(
        hover["text"].as_str().is_some_and(|t| t.contains("Unit")),
        "{hover}"
    );
    let units: Vec<u16> = source.encode_utf16().collect();
    let range = unit(&hover["from"])..unit(&hover["to"]);
    assert_eq!(String::from_utf16_lossy(&units[range]), "main");
    let items = json(&session.complete(at, false));
    assert!(
        items
            .as_array()
            .expect("items")
            .iter()
            .any(|i| i["label"] == "main"),
        "{items}"
    );
}

#[test]
fn attribute_hover_updates_at_utf16_offsets() {
    let source = "module Main exposing (..)\nlet text = \"ёж 🦔\"\n@lang action_concurrent\nlet concurrent = ()\n";
    let mut session = session(source);
    for (text, word, expected) in [
        (source.to_owned(), "action_concurrent", "Action.concurrent"),
        (
            source.replace("@lang action_concurrent", "@maker"),
            "@maker",
            "atom, action or view",
        ),
    ] {
        session.update(&text);
        let start = text.find(word).expect("attribute");
        let at = u32::try_from(text[..start].encode_utf16().count()).expect("small");
        let hover = json(&session.hover(at));
        assert!(
            hover["text"].as_str().is_some_and(|s| s.contains(expected)),
            "{hover}"
        );
        assert_eq!(hover["from"], at);
        assert_eq!(hover["to"], at + u32::try_from(word.len()).expect("small"));
    }
}

#[test]
fn format_and_explain_need_no_session() {
    let formatted = json(&flick_wasm::format(
        "module Main exposing (x)\nx : Int\nlet x =   1\n",
    ));
    assert_eq!(
        formatted["text"],
        "module Main exposing (x)\nx : Int\nlet x = 1\n"
    );
    assert!(json(&flick_wasm::format("module Main exposing (\n"))["diagnostics"].is_array());
    assert!(flick_wasm::explain("FLICK-E002").contains("signature"));
    assert_eq!(flick_wasm::explain("FLICK-X999"), "");
}

/// An export the `.d.ts` cannot write says nothing about the program: the
/// build goes on and the note stays with the declarations.
#[test]
fn what_the_dts_leaves_out_is_kept_apart() {
    let source = "module Main exposing (number)\n\ntype UserId = UserId Int\n\nnumber : UserId -> Int\nlet number id =\n  match id with\n  | UserId n -> n\n";
    let emitted = json(&session(source).emit("debug"));
    assert_eq!(
        emitted["diagnostics"],
        Value::Array(Vec::new()),
        "{emitted}"
    );
    assert!(emitted["js"].is_string(), "{emitted}");
    assert_eq!(
        emitted["dts_diagnostics"][0]["code"], "FLICK-F004",
        "{emitted}"
    );
}

#[test]
fn reexport_declarations_are_available_on_demand() {
    let session = session(
        "module Main exposing (main, length)\nimport String exposing (length)\nmain : Unit ->{} Unit\nlet main () = Platform.stdout_line (show (length \"Flick\"))\n",
    );
    let emitted = json(&session.emit_declarations("debug"));
    assert_eq!(emitted["declaration_entry"], "Main.d.ts");
    assert!(
        emitted["dts"]
            .as_str()
            .expect("entry declarations")
            .contains("export { length }")
    );
    assert!(
        emitted["declarations"]["_flick/String.d.ts"]
            .as_str()
            .expect("String declarations")
            .contains("export declare const length: (arg: string) => number;")
    );
    assert!(
        json(&session.emit("debug"))["declarations"].is_null(),
        "normal editor emits keep their existing payload"
    );
}
