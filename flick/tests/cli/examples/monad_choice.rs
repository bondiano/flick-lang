use crate::common;
use std::fs;

use super::{built_main, success};

#[test]
fn the_monad_choice_example_checks_and_is_formatted() {
    let dir = common::example("monad-choice", "examples/monad-choice-check");
    success(&dir, &["check", "--warnings-as-errors"]);
    success(&dir, &["fmt", "--check"]);
}

#[test]
#[ignore = "needs Node and the installed runtime"]
fn the_monad_choice_example_runs_and_solves_plans() {
    monad_choice_runs("js");
}

#[test]
#[ignore = "needs Go"]
fn the_monad_choice_example_runs_and_solves_plans_on_go() {
    monad_choice_runs("go");
}

fn monad_choice_runs(emit: &str) {
    let dir = common::example("monad-choice", &format!("examples/monad-choice-{emit}"));
    success(&dir, &["test", "--target", emit, "--seed", "7"]);
    success(&dir, &["build", "--target", emit]);
    let solved = built_main(&dir, emit)
        .args([
            "solve",
            "--tier",
            "production",
            "--max-cost",
            "450",
            "--max-latency",
            "35",
            "--min-sla",
            "9995",
            "--zero-downtime",
        ])
        .output()
        .expect("the solver runs");
    assert!(solved.status.success(), "{solved:?}");
    let expected = fs::read_to_string(dir.join("expected.stdout")).expect("expected.stdout");
    assert_eq!(String::from_utf8_lossy(&solved.stdout), expected);

    let matrix = built_main(&dir, emit)
        .args(["matrix", "--tier", "preview"])
        .output()
        .expect("the matrix runs");
    assert!(matrix.status.success(), "{matrix:?}");
    assert!(
        String::from_utf8_lossy(&matrix.stdout).starts_with(
            "Tier: preview (26 valid release plans)
"
        ),
        "{matrix:?}"
    );

    let unsat = built_main(&dir, emit)
        .args([
            "solve",
            "--tier",
            "production",
            "--max-cost",
            "100",
            "--zero-downtime",
        ])
        .output()
        .expect("the unsatisfiable solver runs");
    assert_eq!(unsat.status.code(), Some(1), "{unsat:?}");
    assert!(
        String::from_utf8_lossy(&unsat.stderr).contains("after 73 choice steps and 74 backtracks"),
        "{unsat:?}"
    );
}
