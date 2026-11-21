//! Programs of the target sizes reach the official components and run
//! (#705): an expression nested deeper than a CBOR decoder's default limit
//! of 256 levels, and a 2000-line function optimized in a release build.

use std::fmt::Write as _;
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

/// `main` prints the last of `lines` local bindings, each the one before
/// plus its index: 1 + (1 + … + lines - 1).
fn long_main(lines: usize) -> (String, String) {
    let mut source =
        String::from("module App exposing (main)\nrun : Float -> Float\nlet run x0 =\n");
    for i in 1..lines {
        let _ = writeln!(source, "  let x{i} = x{} + {i}", i - 1);
    }
    let _ = writeln!(source, "  x{}", lines - 1);
    source.push_str(
        "main : Unit ->{Host.Console} Unit\nlet main () = Platform.stdout_line (show (run 1))\n",
    );
    (source, format!("{}\n", 1 + lines * (lines - 1) / 2))
}

#[test]
#[ignore = "requires Go and the JS runtime; run `just e2e-tests`"]
fn programs_of_the_target_sizes_run_on_both_official_components() {
    let cases = [
        ("deep", deep_main(120), "debug"),
        ("long", long_main(2000), "release"),
    ];
    for emit in ["js", "go"] {
        for (name, (source, expected), profile) in &cases {
            let config =
                format!("{CONFIG}[build]\ntarget = \"{emit}\"\n[tools]\n\"flick/go\" = [\"go\"]\n");
            let root = project(
                &format!("{name}_program_{emit}"),
                &[("flick.toml", &config), ("src/App.fl", source)],
            );
            let run = flick_in(&root, &["run", "--offline", "--profile", profile]);
            assert!(run.status.success(), "{name} on {emit}: {run:?}");
            let stdout = String::from_utf8(run.stdout).expect("UTF-8");
            assert_eq!(&stdout, expected, "{name} on {emit}");
            fs::remove_dir_all(root).expect("cleanup");
        }
    }
}
