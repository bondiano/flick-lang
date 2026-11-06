//! The model checks the programs of `tests/run/model.txt` and runs them,
//! printing what the JS and Go builds print: each is in
//! `tests/run/portable.txt`, whose cases `just e2e` compares on both
//! targets with the same `.stdout`.

use std::path::{Path, PathBuf};
use std::{env, fs};

use flick_model::eval::Outcome;
use flick_model::run;

/// Steps a case may take: enough for every case listed, few enough that a
/// loop in the model fails fast.
const FUEL: u64 = 50_000_000;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run")
}

fn cases(list: &str) -> Vec<String> {
    let text = fs::read_to_string(root().join(list)).expect("readable list of cases");
    text.lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

#[test]
#[ignore = "runs every case of tests/run/model.txt; run `just model`"]
fn model_prints_what_the_builds_print() {
    let portable = cases("portable.txt");
    let only = env::var("FLICK_MODEL_ONLY").ok();
    let mut failures = Vec::new();
    for case in cases("model.txt") {
        assert!(
            portable.contains(&case),
            "`{case}` is in model.txt but not in portable.txt"
        );
        if only.as_ref().is_some_and(|o| !case.contains(o.as_str())) {
            continue;
        }
        let file = root().join(format!("{case}.fl"));
        let expected = fs::read_to_string(root().join(format!("{case}.stdout")))
            .expect("every case has a `.stdout`");
        let modules = match run::compile(&file) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{case}: does not compile:\n{e}"));
                continue;
            }
        };
        let program = run::program(&modules);
        let errors = run::check(&modules, &program);
        if !errors.is_empty() {
            failures.push(format!(
                "{case}: the model rejects it:\n{}",
                errors.join("\n")
            ));
            continue;
        }
        match run::main_of(&modules).map(|main| run::run_main(&program, &main, FUEL)) {
            Err(e) => failures.push(format!("{case}: {e}")),
            Ok(ran) => {
                let ended = match &ran.outcome {
                    Outcome::Value(_) | Outcome::Raise(_) | Outcome::Die(_) => None,
                    other => Some(format!("{other:?}")),
                };
                if let Some(why) = ended {
                    failures.push(format!(
                        "{case}: the model stopped: {why}\nprinted:\n{}",
                        ran.stdout
                    ));
                } else if ran.stdout != expected {
                    failures.push(format!(
                        "{case}: printed\n{}\nexpected\n{expected}",
                        ran.stdout
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// Handlers of `control` operations, which only the Go build runs yet:
/// the model prints what it prints, and a second `resume` is a defect.
#[test]
#[ignore = "runs a program of tests/handlers; run `just model`"]
fn control_handlers_print_what_go_prints() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/handlers/control.fl");
    let modules = run::compile(&file).expect("control.fl compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "the model rejects it:\n{}",
        errors.join("\n")
    );
    let main = run::main_of(&modules).expect("a main");
    let ran = run::run_main(&program, &main, FUEL);
    assert_eq!(
        ran.stdout,
        "[1, 2, 3]\n[3, 2, 1]\nscope closed\n7\n[1, 3]\n"
    );
    assert!(
        matches!(&ran.outcome, Outcome::Die(why) if why.contains("`resume` ran twice")),
        "{:?}",
        ran.outcome
    );
}
