//! `flick doc --check-examples` executes package documentation examples: a
//! runnable example runs against the package's dependencies and a runtime
//! failure fails the documentation gate.

use crate::common;

use std::process::Output;

use common::{flick_in, project};

const LIB: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n";

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// A page whose example declares `module Lib.Demo exposing (main)` with `body`
/// as its program.
fn page(body: &str) -> String {
    format!(
        "module Lib.Api exposing (answer)\n/// ```flick\n/// module Lib.Demo exposing (main)\n///\n{body}\n/// ```\nanswer : Int\nlet answer = 42\n"
    )
}

#[test]
fn doc_runs_an_example_that_succeeds() {
    let dir = project(
        "doc_run_ok",
        &[
            ("flick.toml", LIB),
            (
                "src/Lib/Api.fl",
                &page(
                    "/// main : Unit -> Unit\n/// let main () =\n///   let value = 1 + 1\n///   ()",
                ),
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn doc_fails_an_example_that_throws_at_runtime() {
    let dir = project(
        "doc_run_throw",
        &[
            ("flick.toml", LIB),
            (
                "src/Lib/Api.fl",
                &page(
                    "/// read : String -> String\n/// foreign pure read from \"node:fs\" \"readFileSync\"\n///\n/// main : Unit -> Unit\n/// let main () =\n///   let text = read \"/flick-doc-run/missing\"\n///   ()",
                ),
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_ne!(out.status.code(), Some(0), "{out:?}");
    let stderr = stderr(&out);
    assert!(stderr.contains("failed at runtime"), "{stderr}");
    assert!(stderr.contains("ENOENT"), "{stderr}");
}

#[test]
fn doc_runs_an_example_against_a_declared_dependency() {
    let dir = project(
        "doc_run_dependency",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\nhelper = { path = \"helper\" }\n",
            ),
            ("helper/flick.toml", LIB),
            (
                "helper/src/Acme/Text.fl",
                "module Acme.Text exposing (trim)\ntrim : String -> String\nlet trim s = s\n",
            ),
            (
                "src/Lib/Api.fl",
                &page(
                    "/// import Acme.Text exposing (trim)\n///\n/// main : Unit -> Unit\n/// let main () =\n///   let text = trim \"ok\"\n///   ()",
                ),
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
fn doc_reports_an_undeclared_import_beside_a_declared_one() {
    let dir = project(
        "doc_run_undeclared_beside_declared",
        &[
            (
                "flick.toml",
                "schema-version = 1\n[project]\nkind = \"lib\"\n[dependencies]\nother = { path = \"other\" }\n",
            ),
            ("other/flick.toml", LIB),
            (
                "other/src/Other/Thing.fl",
                "module Other.Thing exposing (x)\nx : Int\nlet x = 1\n",
            ),
            ("helper/flick.toml", LIB),
            (
                "helper/src/Acme/Text.fl",
                "module Acme.Text exposing (trim)\ntrim : String -> String\nlet trim s = s\n",
            ),
            (
                "src/Lib/Api.fl",
                &page(
                    "/// import Acme.Text exposing (trim)\n///\n/// main : Unit -> Unit\n/// let main () =\n///   let text = trim \"ok\"\n///   ()",
                ),
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_ne!(out.status.code(), Some(0), "{out:?}");
    let stderr = stderr(&out);
    assert!(
        stderr.contains("imports `Acme`, which is not a declared dependency"),
        "{stderr}"
    );
}

#[test]
fn doc_reports_an_import_of_an_undeclared_dependency() {
    let dir = project(
        "doc_run_undeclared",
        &[
            ("flick.toml", LIB),
            ("helper/flick.toml", LIB),
            (
                "helper/src/Acme/Text.fl",
                "module Acme.Text exposing (trim)\ntrim : String -> String\nlet trim s = s\n",
            ),
            (
                "src/Lib/Api.fl",
                &page(
                    "/// import Acme.Text exposing (trim)\n///\n/// main : Unit -> Unit\n/// let main () =\n///   let text = trim \"ok\"\n///   ()",
                ),
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_ne!(out.status.code(), Some(0), "{out:?}");
    let stderr = stderr(&out);
    assert!(
        stderr.contains("imports `Acme`, which is not a declared dependency"),
        "{stderr}"
    );
}
