//! A program whose expressions nest deeper than a CBOR decoder's default
//! limit of 256 levels still reaches the official components (#705).

use std::fs;

use super::CONFIG;
use super::common::{flick_in, project};

/// `main` prints a concatenation of `n` strings, a chain `n` levels deep.
fn deep_main(n: usize) -> (String, String) {
    let terms: Vec<String> = (0..n).map(|i| format!("\"{}\"", i % 10)).collect();
    let source = format!(
        "module App exposing (main)\nmain : Unit ->{{Host.Console}} Unit\nlet main () = Platform.stdout_line ({})\n",
        terms.join(" ++ ")
    );
    let expected: String = (0..n).map(|i| (i % 10).to_string()).collect();
    (source, format!("{expected}\n"))
}

#[test]
#[ignore = "requires Go and the JS runtime; run `just e2e-tests`"]
fn a_deeply_nested_program_builds_and_runs_on_both_official_components() {
    let (source, expected) = deep_main(120);
    for emit in ["js", "go"] {
        let config =
            format!("{CONFIG}[build]\nemit = \"{emit}\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
        let root = project(
            &format!("deep_program_{emit}"),
            &[("flick.toml", &config), ("src/App.fl", &source)],
        );
        let run = flick_in(&root, &["run", "--offline"]);
        assert!(run.status.success(), "{emit}: {run:?}");
        assert_eq!(
            String::from_utf8(run.stdout).expect("UTF-8"),
            expected,
            "{emit}"
        );
        fs::remove_dir_all(root).expect("cleanup");
    }
}
