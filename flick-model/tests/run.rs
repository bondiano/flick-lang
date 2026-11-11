//! The model checks the programs of `tests/run/model.txt` and runs them,
//! printing what the JS and Go builds print: each is in
//! `tests/run/portable.txt`, whose cases `just e2e` compares on both
//! targets with the same `.stdout`.
//! Every other case of `tests/run` is in `tests/run/model_gaps.txt` with what
//! stops the model, so a case is run here or says why it is not.

use std::path::{Path, PathBuf};
use std::{env, fs, panic};

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
fn module_dictionaries_are_checked_and_run_in_the_model() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/modules/App/DictionaryMain.fl");
    let modules = run::compile(&file).expect("module fixture compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "model rejects the module fixture: {errors:?}"
    );
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, FUEL);
    assert_eq!(ran.stdout, "ok\n", "{:?}", ran.outcome);
}

#[test]
fn higher_order_modules_erase_to_checked_core() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/modules/App/Hom/Main.fl");
    let modules = run::compile(&file).expect("higher-order fixture compiles");
    let templates: Vec<_> = modules
        .iter()
        .filter(|m| m.interface.parameterized.is_some())
        .collect();
    assert!(
        !templates.is_empty(),
        "fixture must include static templates"
    );
    assert!(
        templates.iter().all(|m| m.ir.items.is_empty()),
        "unapplied templates have no executable core items"
    );
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "model rejects the higher-order fixture: {errors:?}"
    );
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, FUEL);
    assert_eq!(
        ran.stdout, "42 false false false true\n",
        "{:?}",
        ran.outcome
    );
    assert!(
        matches!(ran.outcome, Outcome::Value(_)),
        "{:?}",
        ran.outcome
    );
}

#[test]
fn existential_module_witnesses_run_in_the_model() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/modules/App/Main.fl");
    let modules = run::compile(&file).expect("module fixture compiles");
    let program = run::program(&modules);
    // The core model has records and nominal types, but no existential
    // introduction/elimination rules. The source checker establishes the
    // witness type; this checks its elaborated runtime representation.
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, FUEL);
    assert_eq!(ran.stdout, "ok\n", "{:?}", ran.outcome);
}

#[test]
fn inline_module_contract_pack_runs_in_the_model() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/run/bootstrap/inline_pack_nominals.fl");
    let modules = run::compile(&file).expect("inline module fixture compiles");
    let program = run::program(&modules);
    // The core model checks the elaborated runtime record. It does not yet
    // type-check source-level `Module Sig` packing or opening.
    let main = run::main_of(&modules).expect("main");
    let ran = run::run_main(&program, &main, FUEL);
    assert_eq!(ran.stdout, "hello Ada\n", "{:?}", ran.outcome);
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
        "[1, 2, 3]\n[3, 2, 1]\nscope closed\n7\n[1, 3]\n(7, 7)\nlast\nfirst\n19\n17\n11\n9\n(2, 7)\n(5, 5)\n(3, 7)\n"
    );
    assert!(
        matches!(&ran.outcome, Outcome::Die(why) if why.contains("`resume` ran twice")),
        "{:?}",
        ran.outcome
    );
}

/// A child reaches the handlers around where it started, as on both targets.
#[test]
#[ignore = "runs a program of tests/handlers; run `just model`"]
fn a_child_reaches_the_handlers_around_it() {
    let file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/handlers/children.fl");
    let modules = run::compile(&file).expect("children.fl compiles");
    let program = run::program(&modules);
    let errors = run::check(&modules, &program);
    assert!(
        errors.is_empty(),
        "the model rejects it:\n{}",
        errors.join("\n")
    );
    let main = run::main_of(&modules).expect("a main");
    let ran = run::run_main(&program, &main, FUEL);
    assert_eq!(ran.stdout, "record\narm\n", "{:?}", ran.outcome);
}

/// Every case of `tests/run`, by path without `.fl`.
fn every_case() -> Vec<String> {
    let mut found = Vec::new();
    let mut dirs = vec![root()];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(dir).expect("a directory of cases").flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "fl") {
                let case = path.strip_prefix(root()).expect("under tests/run");
                found.push(case.with_extension("").to_string_lossy().into_owned());
            }
        }
    }
    found.sort();
    found
}

/// Why the model does not run `case` as the builds do; `None` when it does.
fn gap(case: &str, portable: bool) -> Option<String> {
    let file = root().join(format!("{case}.fl"));
    let Ok(modules) = run::compile(&file) else {
        return Some("does not compile alone".to_owned());
    };
    let program = run::program(&modules);
    // `Module.name: (Rule) message`: a form with no translation says which,
    // a rule of the model's own is named with what it rejects
    if let Some(first) = run::check(&modules, &program).first() {
        let line = first.lines().next().unwrap_or_default();
        let (name, rest) = line.split_once(": (").unwrap_or(("", line));
        let (rule, message) = rest.split_once(") ").unwrap_or((rest, ""));
        return Some(match rule {
            "Translation" => message.to_owned(),
            _ => format!("the model's ({rule}) rejects `{name}`"),
        });
    }
    let Ok(main) = run::main_of(&modules) else {
        return Some("has no `main` the model runs".to_owned());
    };
    let ran = run::run_main(&program, &main, FUEL);
    match &ran.outcome {
        Outcome::Stuck(why) => Some(why.lines().next().unwrap_or_default().to_owned()),
        Outcome::OutOfFuel => Some("runs out of the model's steps".to_owned()),
        _ if fs::read_to_string(root().join(format!("{case}.stdout"))).ok() != Some(ran.stdout) => {
            Some("prints something else".to_owned())
        }
        _ if !portable => Some("runs in the model; not in portable.txt".to_owned()),
        _ => None,
    }
}

/// The cases the model does not run say why, in `tests/run/model_gaps.txt`:
/// a case that begins to run moves to `model.txt`, a new case is run or says
/// why not, and a reason that changes is written again.
#[test]
#[ignore = "runs every case of tests/run in the model; run `just model`"]
fn a_case_the_model_does_not_run_says_why() {
    let (listed, portable) = (cases("model.txt"), cases("portable.txt"));
    let found: Vec<String> = every_case()
        .into_iter()
        .filter(|case| !listed.contains(case))
        .map(|case| {
            let portable = portable.contains(&case);
            let reason = panic::catch_unwind(|| gap(&case, portable))
                .unwrap_or_else(|_| Some("the model panics".to_owned()));
            match reason {
                Some(reason) => format!("{case} # {reason}"),
                None => format!("{case} # runs as the builds do: move it to model.txt"),
            }
        })
        .collect();
    let text = fs::read_to_string(root().join("model_gaps.txt")).expect("the list of gaps");
    let written: Vec<&str> = text
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect();
    let changed: Vec<String> = (found.iter())
        .filter(|line| !written.contains(&line.as_str()))
        .map(|line| format!("+ {line}"))
        .chain(
            (written.iter())
                .filter(|line| !found.iter().any(|f| f == *line))
                .map(|line| format!("- {line}")),
        )
        .collect();
    assert!(
        changed.is_empty(),
        "tests/run/model_gaps.txt is not what the model does:\n{}",
        changed.join("\n")
    );
}
