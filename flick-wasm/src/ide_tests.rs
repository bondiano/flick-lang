//! The editor features of a session: the offsets an editor sees, the
//! diagnostics and the build a reused cache must not change.

use std::fs::{read_dir, read_to_string};
use std::num::NonZero;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use serde_json::Value;

use flick_core::build;
use flick_core::diagnostics::render::json_events;
use flick_core::project::{self, Analysis};
use flick_core::source::SourceMap;
use flick_core::types;

use crate::{Demo, Session, path, positions};

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("JSON")
}

/// The diagnostics of `text` from a run that keeps nothing.
fn cold(text: &str) -> Value {
    let mut sources = SourceMap::default();
    let analysis: Analysis = project::analyze_rt(&Demo(text), &mut sources, None, &[path()]);
    let mut events =
        serde_json::to_value(json_events(&sources, &analysis.diagnostics).collect::<Vec<_>>())
            .expect("diagnostic events serialize");
    positions(text, &mut events);
    events
}

/// The sources of the playground examples, as the site ships them.
fn examples() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/src/playground/examples");
    let mut out: Vec<(String, String)> = read_dir(dir)
        .expect("the playground examples")
        .filter_map(|entry| {
            let entry = entry.expect("entry");
            let path = entry.path();
            (path.extension().is_some_and(|e| e == "fl")).then(|| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    read_to_string(&path).expect("example"),
                )
            })
        })
        .collect();
    out.sort();
    out
}

#[test]
fn offsets_cross_the_boundary_in_utf16_units() {
    // "é" is 2 bytes and 1 UTF-16 unit; "𝄞" is 4 bytes and 2 units, so every
    // offset after the string is 3 less in UTF-16 than in UTF-8
    let source = "module Main exposing (main)\n\ngreeting : String\nlet greeting = \"é𝄞\"\n\nmain : Unit -> Unit\nlet main () =\n  if greeting == \"\" then ()\n";
    let mut session = Session::new();
    assert_eq!(
        json(&session.update(source)),
        Value::Array(Vec::new()),
        "the fixture type checks"
    );

    let tokens = json(&session.semantic_tokens());
    let used = tokens["tokens"]
        .as_array()
        .expect("tokens")
        .iter()
        .find(|token| {
            let from = usize::try_from(token["from"].as_u64().expect("from")).expect("offset");
            let byte = source_after(source, from);
            // the use in `main`, not the declaration
            from > 100 && token["type"] == "variable" && byte.starts_with("greeting")
        })
        .expect("the use of `greeting` is a variable token");
    let from = usize::try_from(used["from"].as_u64().expect("from")).expect("offset");
    let byte_start = source
        .match_indices("greeting")
        .map(|(i, _)| i)
        .find(|&i| i > source.find("main : Unit").expect("main"))
        .expect("the use");
    // the token slice read at the converted offset is the same name
    assert!(
        source_after(source, from).starts_with("greeting"),
        "the UTF-16 offset names the use"
    );
    assert_eq!(from, byte_start - 3, "UTF-16 offset of the use");
    assert_eq!(
        usize::try_from(used["to"].as_u64().expect("to")).expect("offset") - from,
        "greeting".len(),
        "the token covers the whole name"
    );

    // an offset given in UTF-16 units reaches the same byte of the module,
    // and an action's UTF-16 range reaches the same bytes again
    let byte = usize::try_from(crate::byte_offset(
        source,
        u32::try_from(from).expect("offset"),
    ))
    .expect("offset");
    assert_eq!(byte, byte_start, "the round trip is the identity");
    let actions = json(&session.code_actions(
        u32::try_from(from).expect("offset"),
        u32::try_from(from + "greeting".len()).expect("offset"),
    ));
    assert!(actions.is_array(), "{actions}");
}

/// The module text from the UTF-16 offset `at`, decoded, used to check that a
/// converted offset lands on the same name an editor would see.
fn source_after(source: &str, at: usize) -> String {
    let mut units = 0;
    for (byte, ch) in source.char_indices() {
        if units == at {
            return source[byte..].to_owned();
        }
        units += ch.len_utf16();
    }
    String::new()
}

fn for_each_example(examples: &[(String, String)], check: impl Fn(&str, &str) + Sync) {
    let workers = thread::available_parallelism()
        .map_or(1, NonZero::get)
        .min(examples.len());
    if workers <= 1 {
        for (name, source) in examples {
            check(name, source);
        }
        return;
    }
    let next = AtomicUsize::new(0);
    thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                let check = &check;
                let next = &next;
                scope.spawn(move || {
                    loop {
                        let idx = next.fetch_add(1, Ordering::Relaxed);
                        let Some((name, source)) = examples.get(idx) else {
                            break;
                        };
                        check(name, source);
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().expect("example worker passed");
        }
    });
}

#[test]
fn a_reused_cache_reports_what_a_cold_run_reports() {
    let examples = examples();
    let _ = types::builtins_of(true);
    for_each_example(&examples, |name, source| {
        let mut session = Session::new();
        let first = json(&session.update(source));
        assert_eq!(first, cold(source), "{name}: first analysis");

        // an edit and its undo: the cache of the first analysis is reused
        let edited = source.replace(
            "module Main exposing (main)",
            "module Main exposing (main)\n",
        );
        let analysed = json(&session.update(&edited));
        assert_eq!(analysed, cold(&edited), "{name}: after an edit");
        let again = json(&session.update(source));
        assert_eq!(again, cold(source), "{name}: after the undo");
        // the revision count is not in the report, but the cache is: every
        // update goes through the same `ModuleCache` the session owns, which
        // `checking.rs` retains by path, so the second analysis of the same
        // text cannot parse it again
        assert_eq!(again, first, "{name}: the same text analyses the same");
    });
}

#[test]
fn emission_is_the_same_after_the_cache_was_reused() {
    let examples = examples();
    let _ = types::builtins_of(true);
    let _ = build::builtin_modules_of(true);
    for_each_example(&examples, |name, source| {
        let mut session = Session::new();
        session.update(source);
        session.update(&source.replace(
            "module Main exposing (main)",
            "module Main exposing (main)\n",
        ));
        session.update(source);
        let mut fresh = Session::new();
        fresh.update(source);
        for profile in ["debug", "release"] {
            let reused: Value = json(&session.emit(profile));
            let cold: Value = json(&fresh.emit(profile));
            assert_eq!(reused, cold, "{name}: `emit {profile}`");
            let reused: Value = json(&session.emit_declarations(profile));
            let cold: Value = json(&fresh.emit_declarations(profile));
            assert_eq!(reused, cold, "{name}: `emit_declarations {profile}`");
        }
    });
}

#[test]
fn signature_help_reaches_the_call_under_the_parenthesis() {
    let source = "module Main exposing (main)\n\ndouble : Int -> Int\nlet double x = x * 2\n\nmain : Unit -> Unit\nlet main () =\n  let doubled = double (21)\n  if doubled == 42 then ()\n";
    let mut session = Session::new();
    assert_eq!(json(&session.update(source)), Value::Array(Vec::new()));
    let at = source.find("double (21)").expect("the call") + "double (".len();
    let help = json(&session.signature_help(u32::try_from(at).expect("offset")));
    assert!(
        help["label"]
            .as_str()
            .is_some_and(|label| label.contains("Int")),
        "{help}"
    );
}

#[test]
fn let_else_hover_actions_and_emission_work_in_wasm_session() {
    let source = "module Main exposing (main)\n\ngreet : Option String -> String\nlet greet raw =\n  let tag = \"é𝄞:\"\n  let Some name = raw else \"guest\"\n  tag ++ name\n\nmain : Unit -> Unit\nlet main () =\n  if greet (Some \"ada\") == \"\" then ()\n";
    let mut session = Session::new();
    assert_eq!(json(&session.update(source)), Value::Array(Vec::new()));

    // UTF-16 offset of `else` (3 less than UTF-8 byte offset because of `é𝄞`).
    let else_byte = source.find("else \"guest\"").expect("else");
    let else_utf16 = u32::try_from(else_byte - 3).expect("utf16");
    let hover = json(&session.hover(else_utf16));
    assert!(
        hover["text"]
            .as_str()
            .is_some_and(|c| c.contains("String") && c.contains("does not match")),
        "{hover}"
    );

    let let_byte = source.find("let Some name").expect("let");
    let let_utf16 = u32::try_from(let_byte - 3).expect("utf16");
    let actions = json(&session.code_actions(let_utf16, let_utf16 + 3));
    assert!(
        actions
            .as_array()
            .expect("actions")
            .iter()
            .any(|a| a["title"] == "expand `let ... else` to `match`"),
        "{actions}"
    );

    // Edit & undo reuses cache and emits identical debug/release output to a cold session.
    let edited = source.replace("\"guest\"", "\"anon\"");
    assert_eq!(json(&session.update(&edited)), Value::Array(Vec::new()));
    assert_eq!(json(&session.update(source)), Value::Array(Vec::new()));
    let mut cold_session = Session::new();
    cold_session.update(source);
    for profile in ["debug", "release"] {
        assert_eq!(
            json(&session.emit(profile)),
            json(&cold_session.emit(profile))
        );
    }
}

#[test]
fn result_block_wasm_hover_tokens_and_code_actions_respect_utf16_and_edits() {
    let source = "module Main exposing (main)\n\ntype error Boom = Boom\n\nmain : Bool ->{} Result Boom String\nlet main flag =\n  let _ = \"é𝄞\"\n  result { if flag then Fail.fail Boom else \"ok\" }\n";
    let mut session = Session::new();
    assert_eq!(json(&session.update(source)), Value::Array(Vec::new()));

    let byte_at = source.find("result {").expect("result");
    let utf16_at: u32 = source[..byte_at]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>()
        .try_into()
        .expect("utf16 offset");

    let tokens = json(&session.semantic_tokens());
    let has_kw = tokens["tokens"]
        .as_array()
        .expect("tokens")
        .iter()
        .any(|t| {
            t["from"] == u64::from(utf16_at)
                && t["to"] == u64::from(utf16_at + 6)
                && t["type"] == "keyword"
        });
    assert!(
        has_kw,
        "expected keyword semantic token at {utf16_at}: {tokens}"
    );

    let hover = json(&session.hover(utf16_at + 1));
    assert!(
        hover["text"]
            .as_str()
            .is_some_and(|s| s.contains("result { … } : Result Boom String")),
        "{hover}"
    );

    let actions = json(&session.code_actions(utf16_at, utf16_at + 6));
    assert!(
        actions.as_array().is_some_and(|arr| arr.iter().any(|a| {
            a["title"]
                .as_str()
                .is_some_and(|t| t.contains("expand `result"))
        })),
        "{actions}"
    );

    // Edit into canonical `try` and ensure cache updates cleanly and offers forward rewrite
    let edited = "module Main exposing (main)\n\ntype error Boom = Boom\n\nmain : Bool ->{} Result Boom String\nlet main flag =\n  let _ = \"é𝄞\"\n  try if flag then Fail.fail Boom else \"ok\" with\n  | return x -> Ok x\n  | e -> Err e\n";
    assert_eq!(json(&session.update(edited)), Value::Array(Vec::new()));
    let try_byte = edited.find("try if").expect("try");
    let try_utf16: u32 = edited[..try_byte]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>()
        .try_into()
        .expect("utf16");
    let edited_actions = json(&session.code_actions(try_utf16, try_utf16 + 3));
    assert!(
        edited_actions
            .as_array()
            .is_some_and(|arr| arr.iter().any(|a| {
                a["title"]
                    .as_str()
                    .is_some_and(|t| t.contains("rewrite as `result"))
            })),
        "{edited_actions}"
    );
}
