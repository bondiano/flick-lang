//! Direct compiler-worker emission and the editor's lazy check path must
//! agree with the original cold emit, including across invalid revisions.

use std::sync::Arc;

use serde_json::Value;

use crate::{Session, path};

const SOURCE: &str = "module Main exposing (answer, main)\n// привет 😀\nanswer : String\nlet answer = \"first\"\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line answer\n";

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("JSON")
}

#[test]
fn direct_emission_matches_cold_after_edits_errors_and_fixes() {
    let edited = SOURCE.replace("first", "second 😀");
    let invalid = edited.replace("\"second 😀\"", "42");
    let fixed = SOURCE.replace("first", "fixed 🎉");
    let mut session = Session::new();
    for source in [SOURCE, &edited, &invalid, &fixed] {
        session.prepare_emit(source);
        let mut cold = Session::new();
        let checked = json(&cold.update(source));
        for profile in ["debug", "release"] {
            let actual = json(&session.emit(profile));
            assert_eq!(actual, json(&cold.emit(profile)), "{profile}: {source}");
            assert_eq!(
                json(&session.emit_declarations(profile)),
                json(&cold.emit_declarations(profile)),
                "declarations, {profile}: {source}"
            );
            assert_eq!(actual["diagnostics"], checked);
            if source == invalid {
                for field in ["js", "dts", "files", "entry"] {
                    assert!(actual[field].is_null(), "no stale {field}: {actual}");
                }
                let event = actual["diagnostics"]
                    .as_array()
                    .expect("diagnostics")
                    .iter()
                    .find(|event| event["severity"] == "error")
                    .expect("the invalid revision has an error");
                let start = source.find("42").expect("bad value");
                let start = source[..start].encode_utf16().count();
                assert_eq!(event["from"], start);
                assert_eq!(event["to"], start + 2);
            } else {
                assert!(actual["js"].is_string());
                assert_eq!(actual["entry"], "flick-main.js");
            }
        }
        // An unchanged update/query after Run belongs to the just-built
        // revision, including its non-BMP prefix and fixed diagnostics.
        assert_eq!(json(&session.update(source)), checked);
        let at = source.rfind("answer").expect("use");
        let at = u32::try_from(source[..at].encode_utf16().count()).expect("offset");
        assert_eq!(json(&session.hover(at)), json(&cold.hover(at)));
        assert_eq!(
            json(&session.complete(at, false)),
            json(&cold.complete(at, false))
        );
    }
}

#[test]
fn direct_emission_drops_removed_exports_and_runners() {
    let library = "module Renamed exposing (answer)\nanswer : Int\nlet answer = 7\n";
    let mut session = Session::new();
    session.prepare_emit(SOURCE);
    assert!(json(&session.emit("debug"))["files"]["Main.js"].is_string());
    session.prepare_emit(library);
    let mut cold = Session::new();
    cold.update(library);
    for profile in ["debug", "release"] {
        let actual = json(&session.emit(profile));
        assert_eq!(actual, json(&cold.emit(profile)));
        assert!(actual["entry"].is_null());
        assert!(actual["files"]["Main.js"].is_null());
        assert!(actual["files"]["Renamed.js"].is_string());
    }
}

#[test]
fn empty_source_never_reuses_an_earlier_program() {
    let mut session = Session::new();
    for source in ["", SOURCE, "", "module Main exposing (\n", "", SOURCE] {
        session.prepare_emit(source);
        let mut cold = Session::new();
        cold.update(source);
        for profile in ["debug", "release"] {
            assert_eq!(json(&session.emit(profile)), json(&cold.emit(profile)));
        }
    }
}

#[test]
fn direct_preparation_releases_the_previous_editor_snapshot() {
    let mut session = Session::new();
    session.update(SOURCE);
    let old = Arc::downgrade(&session.analysis.module("Main").expect("Main").typing);
    session.prepare_emit("module Main exposing (answer)\nlet answer = 7\n");
    assert!(
        old.upgrade().is_none(),
        "a direct build releases the old checked cache"
    );
}

#[test]
fn checks_remain_lazy_and_profiles_share_the_prepared_revision() {
    let mut session = Session::new();
    session.update(SOURCE);
    assert!(!session.prepared);
    assert!(session.emitted.borrow().is_none());
    assert!(session.analysis.compilation.modules.is_empty());

    // A check followed by a Run still creates typed IR only when requested.
    session.prepare_emit(SOURCE);
    assert!(!session.prepared);
    assert!(session.emitted.borrow().is_some());
    assert!(json(&session.emit("debug"))["js"].is_string());

    let changed = SOURCE.replace("first", "direct");
    session.prepare_emit(&changed);
    assert!(session.prepared);
    assert!(session.emitted.borrow().is_none());
    let typing = session
        .analysis
        .module("Main")
        .expect("Main")
        .typing
        .clone();
    assert!(
        session
            .analysis
            .compilation
            .modules
            .iter()
            .any(|m| m.path == path())
    );
    for profile in ["debug", "release", "debug"] {
        session.prepare_emit(&changed);
        assert!(json(&session.emit(profile))["js"].is_string());
        assert!(Arc::ptr_eq(
            &typing,
            &session.analysis.module("Main").expect("Main").typing
        ));
        assert!(session.emitted.borrow().is_none(), "no second analysis");
    }

    session.update(SOURCE);
    assert!(!session.prepared);
    assert!(session.emitted.borrow().is_none());
    assert!(session.analysis.compilation.modules.is_empty());
}
