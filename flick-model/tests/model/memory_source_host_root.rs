//! The counted executor gives a runner-less `main` the live `Host` services
//! as its root, `Σ₀`, as the evaluator does: an operation no frame answers
//! is answered by `Host.live`, and a provider of `main` still wins.
use std::{fs, path::Path};

use flick_model::memory::{Heap, ProgramOutcome};
use flick_model::run;

#[test]
fn a_runner_less_main_gets_the_host_root_in_both_profiles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    for case in ["effect/host_root", "bootstrap/stderr"] {
        let modules = run::compile(&root.join(format!("{case}.fl"))).expect("source checks");
        let main = run::main_of(&modules).expect("main export");
        let expected =
            fs::read_to_string(root.join(format!("{case}.stdout"))).expect("specified stdout");
        for release in [false, true] {
            let ran = run::run_main_counted(&modules, &main, 1_000_000, release)
                .unwrap_or_else(|error| panic!("{case} release={release}: {error}"));
            assert_eq!(ran.stdout, expected, "{case} release={release}");
            assert_eq!(ran.memory.live_objects, 0);
            assert_eq!(ran.memory.allocations, ran.memory.frees);
        }
    }
}

#[test]
fn the_root_answers_only_the_host_services() {
    let modules = run::compile_source(
        r#"module App exposing (main)
import Host
service Other =
  read : Unit ->{} String
main : Unit ->{Host.Console, Other} Unit
let main () =
  Host.Console.stdout_line "host"
  Host.Console.stdout_line (Other.read ())
"#,
    )
    .expect("source checks");
    let main = run::main_of(&modules).expect("main export");
    for release in [false, true] {
        let mut heap = Heap::new();
        let report = run::run_main_counted_outcome(&mut heap, &modules, &main, 100_000, release)
            .expect("counted lowering");
        assert_eq!(report.stdout, "host\n", "release={release}");
        assert!(
            matches!(report.outcome, ProgramOutcome::Failed(_)),
            "release={release}: {:?}",
            report.outcome
        );
        report
            .outcome
            .release(&mut heap)
            .expect("the cause is released");
        let stats = heap.finish().expect("the root layer is released");
        assert_eq!(stats.allocations, stats.frees);
        // the defect is the missing handler, after cleanup without a leak
        assert_eq!(
            run::run_main_counted(&modules, &main, 100_000, release).err(),
            Some("no handler of App.Other.read is in effect".to_owned()),
            "release={release}"
        );
    }
}
