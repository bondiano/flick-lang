//! `flick doc --check-examples` compiles and executes package documentation
//! examples through the selected backend (or `--target <NAME>`) and reports
//! diagnostics with spans inside the original `///` comments.

use crate::common;

use std::env;
use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{flick_in, project};

const LIB: &str = "schema-version = 1\n[project]\nkind = \"lib\"\n";
const GO_LIB: &str = "schema-version = 1\n[tools]\n\"flick/go\" = [\"go\"]\n[project]\nkind = \"lib\"\n[build]\ntarget = \"go\"\n";

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
    assert!(!dir.join("src/Lib/Demo.fl").exists());
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
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = stderr(&out);
    assert!(stderr.contains("FLICK-P061"), "{stderr}");
    assert!(stderr.contains("failed at runtime"), "{stderr}");
    assert!(stderr.contains("ENOENT"), "{stderr}");
    assert!(
        stderr.contains("src/Lib/Api.fl:"),
        "expected source span in src/Lib/Api.fl:\n{stderr}"
    );
    assert!(!dir.join("api/index.md").exists());
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
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = stderr(&out);
    assert!(stderr.contains("FLICK-P062"), "{stderr}");
    assert!(
        stderr.contains("imports `Acme`, which is not a declared dependency"),
        "{stderr}"
    );
    assert!(
        stderr.contains("src/Lib/Api.fl:5:"),
        "expected span at `import Acme.Text` line:\n{stderr}"
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
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let stderr = stderr(&out);
    assert!(stderr.contains("FLICK-P062"), "{stderr}");
    assert!(
        stderr.contains("imports `Acme`, which is not a declared dependency"),
        "{stderr}"
    );
    assert!(
        stderr.contains("src/Lib/Api.fl:5:"),
        "expected span at `import Acme.Text` line:\n{stderr}"
    );
}

#[test]
fn doc_reports_type_parse_and_fail_block_errors_with_source_spans() {
    let type_err_src = "\
module Lib.Api exposing (answer)
/// ```flick
/// # let hidden = 1
/// value : String
/// let value = hidden
/// ```
answer : Int
let answer = 42
";
    let dir = project(
        "doc_span_type_err",
        &[("flick.toml", LIB), ("src/Lib/Api.fl", type_err_src)],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let err = stderr(&out);
    assert!(err.contains("FLICK-T003"), "{err}");
    assert!(
        err.contains("src/Lib/Api.fl:5:"),
        "expected type error span at `let value = hidden` (line 5):\n{err}"
    );
    assert!(!dir.join("api/index.md").exists());

    let fail_unsatisfied_src = "\
module Lib.Api exposing (answer)
/// ```flick fail T003
/// value : Int
/// let value = 1
/// ```
answer : Int
let answer = 42
";
    fs::write(dir.join("src/Lib/Api.fl"), fail_unsatisfied_src).expect("write");
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(out.status.code(), Some(1), "{out:?}");
    let err = stderr(&out);
    assert!(err.contains("FLICK-P062"), "{err}");
    assert!(
        err.contains("src/Lib/Api.fl:3:"),
        "expected P062 span in src/Lib/Api.fl:\n{err}"
    );
}

#[test]
fn doc_runs_large_stdout_example_without_deadlocking() {
    let dir = project(
        "doc_run_large_stdout",
        &[
            ("flick.toml", LIB),
            (
                "src/Lib/Api.fl",
                &page(
                    "/// repeat : Int -> String -> String\n\
                     /// let rec repeat n s =\n\
                     ///   if n <= 0 then s else repeat (n - 1) (s ++ s)\n\
                     ///\n\
                     /// main : Unit ->{Host.Console} Unit\n\
                     /// let main () = Platform.stdout_write (repeat 14 \"0123456789abcdef\")",
                ),
            ),
        ],
    );
    let out = flick_in(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
}

#[test]
#[ignore = "needs Go"]
fn doc_checks_and_runs_examples_on_go_target_without_node() {
    let ok_src = "\
module Lib.Api exposing (answer)
/// ```flick
/// import Lib.Api exposing (answer)
///
/// main : Unit ->{Host.Console} Unit
/// let main () = Platform.stdout_line (Int.to_string answer)
/// ```
answer : Int
let answer = 42
";
    let dir = project(
        "doc_run_go_ok",
        &[("flick.toml", GO_LIB), ("src/Lib/Api.fl", ok_src)],
    );
    let out = flick_in_without_node(&dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(dir.join("api/Lib.Api.md").is_file());

    // A failing Go example reports FLICK-P061 with a span in src/Lib/Api.fl and writes nothing.
    let fail_src = "\
module Lib.Api exposing (answer)
/// ```flick
/// main : Unit ->{; AppError} Unit
/// let main () = App.fail \"go doc boom\"
/// ```
answer : Int
let answer = 42
";
    let fail_dir = project(
        "doc_run_go_fail",
        &[("flick.toml", GO_LIB), ("src/Lib/Api.fl", fail_src)],
    );
    let fail = flick_in_without_node(&fail_dir, &["doc", "--out-dir", "api", "--check-examples"]);
    assert_eq!(fail.status.code(), Some(1), "{fail:?}");
    let err = stderr(&fail);
    assert!(err.contains("FLICK-P061"), "{err}");
    assert!(err.contains("go doc boom"), "{err}");
    assert!(
        err.contains("src/Lib/Api.fl:4:"),
        "expected runtime span at `let main () = App.fail ...`:\n{err}"
    );
    assert!(!fail_dir.join("api/index.md").exists());

    // `--target go` overrides a default JS project's doc example backend.
    let js_with_go_grant =
        "schema-version = 1\n[tools]\n\"flick/go\" = [\"go\"]\n[project]\nkind = \"lib\"\n";
    let override_dir = project(
        "doc_run_target_override",
        &[("flick.toml", js_with_go_grant), ("src/Lib/Api.fl", ok_src)],
    );
    let override_out = flick_in_without_node(
        &override_dir,
        &[
            "doc",
            "--out-dir",
            "api",
            "--check-examples",
            "--target",
            "go",
        ],
    );
    assert_eq!(override_out.status.code(), Some(0), "{override_out:?}");
}
