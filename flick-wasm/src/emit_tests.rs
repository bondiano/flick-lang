//! A persistent compiler-worker session must agree with a fresh cold run,
//! including across invalid revisions and profile changes.

use std::sync::{Arc, Weak};

use serde_json::Value;

use crate::{Session, path};

const SOURCE: &str = "module Main exposing (answer, main)\n// привет 😀\nanswer : String\nlet answer = \"first\"\nmain : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line answer\n";

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("JSON")
}

#[test]
fn suite_entry_is_available_without_main_and_disappears_after_edit() {
    let mut session = Session::new();
    let suite = "module Main\n\nimport Test exposing (expect, to)\n\nsuite \"math\" {\n  test \"sum\" { expect (1 + 1) |> to.equal 2 }\n}\n";
    assert!(
        !session.update(suite).contains("\"error\""),
        "the suite fixture checks"
    );
    for profile in ["debug", "release"] {
        let emitted = json(&session.emit(profile));
        assert_eq!(
            emitted["tests"],
            serde_json::json!({"module": "Main", "entry": "Main.js"})
        );
        assert!(emitted["entry"].is_null());
        assert!(
            emitted["files"]["Main.js"]
                .as_str()
                .expect("module")
                .contains("export const tests")
        );
    }
    session.update(SOURCE);
    assert!(json(&session.emit("debug"))["tests"].is_null());
    session.update("module Main\nlet broken = missing\n");
    assert!(json(&session.emit("debug"))["tests"].is_null());
}

#[test]
fn emission_matches_cold_after_edits_errors_and_fixes() {
    let edited = SOURCE.replace("first", "second 😀");
    let invalid = edited.replace("\"second 😀\"", "42");
    let fixed = SOURCE.replace("first", "fixed 🎉");
    let mut session = Session::new();
    for source in [SOURCE, &edited, &invalid, &fixed] {
        session.update(source);
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
fn emission_drops_removed_exports_and_runners() {
    let library = "module Renamed exposing (answer)\nanswer : Int\nlet answer = 7\n";
    let mut session = Session::new();
    session.update(SOURCE);
    assert!(json(&session.emit("debug"))["files"]["Main.js"].is_string());
    session.update(library);
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
        session.update(source);
        let mut cold = Session::new();
        cold.update(source);
        for profile in ["debug", "release"] {
            assert_eq!(json(&session.emit(profile)), json(&cold.emit(profile)));
        }
    }
}

#[test]
fn a_changed_check_releases_the_previous_emitted_revision() {
    let mut session = Session::new();
    session.update(SOURCE);
    assert!(json(&session.emit("debug"))["js"].is_string());
    let old = {
        let emitted = session.emitted.borrow();
        Arc::downgrade(
            &emitted
                .as_ref()
                .expect("emitted")
                .analysis
                .module("Main")
                .expect("Main")
                .typing,
        )
    };
    let invalid = SOURCE.replace("\"first\"", "42");
    assert!(
        json(&session.update(&invalid))
            .as_array()
            .expect("diagnostics")
            .iter()
            .any(|d| d["severity"] == "error")
    );
    assert!(
        old.upgrade().is_none(),
        "a changed check releases the old emitted analysis before another Run"
    );
    assert!(session.emitted.borrow().is_none());
}

#[test]
fn checks_remain_lazy_and_profiles_share_the_emitted_revision() {
    let mut session = Session::new();
    session.update(SOURCE);
    assert!(session.emitted.borrow().is_none());
    assert!(session.analysis.compilation.modules.is_empty());

    assert!(json(&session.emit("debug"))["js"].is_string());
    let typing = {
        let emitted = session.emitted.borrow();
        let analysis = &emitted.as_ref().expect("emitted").analysis;
        assert!(
            analysis
                .compilation
                .modules
                .iter()
                .any(|m| m.path == path())
        );
        Arc::downgrade(&analysis.module("Main").expect("Main").typing)
    };
    for profile in ["debug", "release", "debug"] {
        // Queries for the same revision must not invalidate its emitted IR.
        session.update(SOURCE);
        assert!(json(&session.emit(profile))["js"].is_string());
        let emitted = session.emitted.borrow();
        assert!(Weak::ptr_eq(
            &typing,
            &Arc::downgrade(
                &emitted
                    .as_ref()
                    .expect("emitted")
                    .analysis
                    .module("Main")
                    .expect("Main")
                    .typing
            )
        ));
    }

    session.update(&SOURCE.replace("first", "changed"));
    assert!(typing.upgrade().is_none());
    assert!(session.emitted.borrow().is_none());
    assert!(session.analysis.compilation.modules.is_empty());
}
