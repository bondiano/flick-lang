use super::Session;
use serde_json::Value;

#[test]
fn playground_compiles_and_queries_an_inline_module() {
    let source = "module Main exposing (main)\nmodule Child exposing (answer) =\n  let answer = 42\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show Child.answer)\n";
    let mut session = Session::new();
    let diagnostics: Value = serde_json::from_str(&session.update(source)).expect("JSON");
    assert_eq!(diagnostics, serde_json::json!([]));
    let built: Value = serde_json::from_str(&session.emit("debug")).expect("JSON");
    assert!(
        built["js"]
            .as_str()
            .is_some_and(|js| js.contains("Child$answer")),
        "{built}"
    );
    let at = u32::try_from(source.find("Child.answer").expect("use") + 6).expect("offset");
    let hover: Value = serde_json::from_str(&session.hover(at)).expect("JSON");
    assert!(
        hover["text"]
            .as_str()
            .is_some_and(|text| text.contains("Int")),
        "{hover}"
    );
}

#[test]
fn a_playground_program_runs_on_flicks_own_runtime() {
    let mut session = Session::new();
    session.update("module Main exposing (main)\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line \"hi\"\n");
    let built: Value = serde_json::from_str(&session.emit("debug")).expect("JSON");
    let runner = built["files"]["flick-main.js"].as_str().expect("a runner");
    assert!(
        runner.contains("from \"@flick-lang/runtime/rt\""),
        "{runner}"
    );
    assert!(!runner.contains("@flick-lang/effect-ts"), "{runner}");
}

#[test]
fn playground_handles_defer_and_errdefer_hover_emit_and_utf16_updates() {
    let mut session = Session::new();
    let src1 = "module Main exposing (main)\nmain : Unit ->{Host.Console} Unit\nlet main () =\n  let _ = \"😀\"\n  scope outer {\n    defer { Platform.stdout_line \"d\" }\n    errdefer { Platform.stdout_line \"e\" }\n  }\n";
    let diags: Value = serde_json::from_str(&session.update(src1)).expect("JSON");
    assert_eq!(diags, serde_json::json!([]));
    let built: Value = serde_json::from_str(&session.emit("debug")).expect("JSON");
    assert!(built["js"].is_string(), "{built}");

    // UTF-16 offset of `defer` after surrogate pair `😀` on earlier line
    let defer_byte = src1.find("defer {").expect("defer");
    let defer_u16 = u32::try_from(src1[..defer_byte].encode_utf16().count()).expect("u16");
    let hover1: Value = serde_json::from_str(&session.hover(defer_u16 + 1)).expect("JSON");
    assert!(
        hover1["text"]
            .as_str()
            .is_some_and(|t| t.contains("`scope outer`") && t.contains("line 5, column 3")),
        "{hover1}"
    );

    // Stale-edit check: wrapping in an inner scope immediately refreshes owner hover
    let src2 = "module Main exposing (main)\nmain : Unit ->{Host.Console} Unit\nlet main () =\n  scope outer {\n    scope inner {\n      errdefer { Platform.stdout_line \"e\" }\n    }\n  }\n";
    let diags2: Value = serde_json::from_str(&session.update(src2)).expect("JSON");
    assert_eq!(diags2, serde_json::json!([]));
    let err_byte = src2.find("errdefer {").expect("errdefer");
    let err_u16 = u32::try_from(src2[..err_byte].encode_utf16().count()).expect("u16");
    let hover2: Value = serde_json::from_str(&session.hover(err_u16 + 2)).expect("JSON");
    assert!(
        hover2["text"]
            .as_str()
            .is_some_and(|t| t.contains("`scope inner`") && t.contains("body fails")),
        "{hover2}"
    );
}
