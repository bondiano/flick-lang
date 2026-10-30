//! A project in a directory whose name has spaces, Cyrillic and an emoji,
//! as gleam's `test/unicode_path ⭐`: `flick build`, `run` and `test` work
//! there on both targets, and a defect still names its `.fl` line, through
//! the source map on Node and `//line` on Go. Ignored by default: part of
//! `just e2e`, needs Go and Node; with `FLICK_E2E_GO_ONLY` only the go half
//! runs, as in the go job of CI.

mod common;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use common::{APP, flick_in, project};

/// Greets its arguments, then divides by their count less one: one
/// argument is a defect on line 5.
const MAIN: &str = "module App.Main exposing (main)

divide : Int -> Int ->{} Int
let divide a b =
  Int.unsafe_div a b

main : Unit ->{; AppError} Unit
let main () =
  let args = Platform.args ()
  Platform.stdout_line (\"привет \" ++ String.join \",\" args)
  ignore (divide 1 (List.length args - 1))
";

const TESTS: &str = "module App.MainTest exposing (tests)

tests : List Test.Test
let tests =
  [
    Test.test \"юникод ⭐\" (fun () -> Test.equal 3 (String.length \"⭐x\" + 1)),
    Test.property \"reverse twice\" (Gen.list 5 (Gen.string 3)) (fun xs -> Test.equal xs (List.reverse (List.reverse xs))),
  ]
";

/// The `.fl` files the build in `dist` maps App.Main to: the
/// `sources` of a JS source map, the paths of Go's `//line` directives.
fn mapped_sources(emit: &str, dist: &Path) -> Vec<PathBuf> {
    if emit == "js" {
        let map: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(dist.join("App/Main.js.map")).expect("map"))
                .expect("a JSON source map");
        let sources = map["sources"].as_array().expect("sources");
        return sources
            .iter()
            .map(|s| dist.join("App").join(s.as_str().expect("a path")))
            .collect();
    }
    let module = dist.join("go/main");
    fs::read_to_string(module.join("m_App_Main.go"))
        .expect("the Go file of App.Main")
        .lines()
        .filter_map(|l| l.strip_prefix("//line ")?.rsplit_once(':'))
        .filter(|(path, _)| Path::new(path).extension().is_some_and(|e| e == "fl"))
        .map(|(path, _)| module.join(path))
        .collect()
}

#[test]
#[ignore = "needs Go and Node"]
fn a_project_builds_runs_and_tests_under_a_non_ascii_path() {
    let dir = project(
        "проект ⭐",
        &[("src/App/Main.fl", MAIN), ("test/App/MainTest.fl", TESTS)],
    );
    let source = fs::canonicalize(dir.join("src/App/Main.fl")).expect("the source");
    let go_only = env::var_os("FLICK_E2E_GO_ONLY").is_some();
    let emits: &[&str] = if go_only { &["go"] } else { &["js", "go"] };
    let mut reports = Vec::new();
    for &emit in emits {
        let config = format!("{APP}test-dir = \"test\"\n\n[build]\nemit = \"{emit}\"\n");
        fs::write(dir.join("flick.toml"), config).expect("write");
        let build = flick_in(&dir, &["build"]);
        assert!(build.status.success(), "{emit}: {build:?}");
        let mapped = mapped_sources(emit, &dir.join("dist"));
        assert!(!mapped.is_empty(), "{emit}: no mapping");
        for path in mapped {
            let resolved = fs::canonicalize(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
            assert_eq!(resolved, source, "{emit}: {path:?}");
        }

        let run = flick_in(&dir, &["run", "--", "a", "b"]);
        assert!(run.status.success(), "{emit}: {run:?}");
        assert_eq!(
            String::from_utf8_lossy(&run.stdout),
            "привет a,b\n",
            "{emit}"
        );
        let died = flick_in(&dir, &["run", "--", "x"]);
        assert_eq!(died.status.code(), Some(1), "{emit}: {died:?}");
        let stderr = String::from_utf8_lossy(&died.stderr);
        assert!(
            stderr.contains("Int.unsafe_div: division by zero"),
            "{emit}: {stderr}"
        );
        assert!(
            stderr.contains("Main.fl:5"),
            "{emit}: the stack names the .fl: {stderr}"
        );
        // Node names the source by its absolute path: the directory survives the source map
        if emit == "js" {
            assert!(stderr.contains("проект ⭐"), "{stderr}");
        }

        let test = flick_in(&dir, &["test", "--seed", "1"]);
        assert!(test.status.success(), "{emit}: {test:?}");
        reports.push(String::from_utf8_lossy(&test.stdout).into_owned());
    }
    assert!(
        reports[0].contains("ok    App.MainTest > юникод ⭐"),
        "{}",
        reports[0]
    );
    assert!(reports.windows(2).all(|w| w[0] == w[1]), "{reports:#?}");
}
