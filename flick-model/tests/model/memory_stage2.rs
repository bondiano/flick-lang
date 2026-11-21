//! Stage 2 counted-execution corpus and bounded-memory stress tests.

use std::path::Path;
use std::{env, fs};

use flick_core::build;
use flick_core::ir::memory::{Name, Program};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_program};
use flick_model::run;
use flick_test_support::entries;

use crate::causes;

fn compile_counted(text: &str, release: bool, entry: &Name) -> Program {
    let modules = run::compile_source(text).expect("source checks");
    let ir: Vec<_> = modules
        .iter()
        .map(|module| {
            if release {
                build::optimized(&modules, module, false)
            } else {
                module.ir.clone()
            }
        })
        .collect();
    perceus::lower_with_declarations(
        ir.iter().chain(build::builtin_modules()),
        modules
            .iter()
            .map(|m| m.declarations.as_ref())
            .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
        entry,
    )
    .expect("Stage 2 counted lowering")
}

#[test]
fn all_model_txt_cases_run_on_counted_path_in_both_profiles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let list = fs::read_to_string(root.join("model.txt")).expect("tests/run/model.txt");
    let cases = entries(&list);
    assert!(
        cases.len() >= 90,
        "expected full model.txt list, got {}",
        cases.len()
    );
    // Stage 4 dynamic `Layer.effect` build phase is intentionally deferred to Stage 4;
    // every other case in `tests/run/model.txt` (89/91, 178 profiles) must pass on the counted path.
    let stage4_layer_effect = ["effect/service_params", "codegen/loops_guards"];
    let mut unexpected_skips = Vec::new();
    let mut ran_count = 0usize;
    for case in cases {
        let path = root.join(format!("{case}.fl"));
        let expected = fs::read_to_string(root.join(format!("{case}.stdout")))
            .expect("independent corpus stdout");
        let modules = run::compile(&path).expect("corpus compiles");
        let main = run::main_of(&modules).expect("main export");
        for release in [false, true] {
            let counted = match run::counted_main(&modules, &main, release) {
                Ok(c) => c,
                Err(err) => {
                    if stage4_layer_effect.contains(&case)
                        && err.contains("Some(\"flick:Layer\"):effect")
                    {
                        continue;
                    }
                    unexpected_skips.push(format!("{case} (release={release}): {err}"));
                    continue;
                }
            };
            let mut heap = Heap::new();
            let report = counted.execute(&mut heap, 2_000_000);
            let outcome_ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
            let stdout = report.stdout.clone();
            let detail = format!("{:?}", report.outcome);
            report
                .outcome
                .release(&mut heap)
                .expect("release main outcome");
            let stats = heap.finish().expect("zero occupied slots");
            assert_eq!(stats.live_objects, 0, "{case} release={release}");
            assert_eq!(stats.allocations, stats.frees, "{case} release={release}");
            assert!(outcome_ok, "{case} release={release}: {detail}");
            assert_eq!(stdout, expected, "{case} release={release}");
            ran_count += 1;
        }
    }
    assert!(
        unexpected_skips.is_empty(),
        "unexpected model.txt skips on counted path:\n{}",
        unexpected_skips.join("\n")
    );
    assert!(ran_count >= 178);
}

#[test]
fn newly_unlocked_stage2_corpus_and_rt_cases_match_stdout_with_zero_leaks() {
    let run_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    for case in [
        "codegen/known_defaults",
        "codegen/for_comprehension",
        "codegen/ranges",
        "runtime/scope_close",
    ] {
        let path = run_root.join(format!("{case}.fl"));
        let expected = fs::read_to_string(run_root.join(format!("{case}.stdout")))
            .expect("independent corpus stdout");
        let modules = run::compile(&path).expect("corpus compiles");
        let main = run::main_of(&modules).expect("main export");
        for release in [false, true] {
            let ran = run::run_main_counted(&modules, &main, 2_000_000, release)
                .unwrap_or_else(|err| panic!("{case} release={release}: {err}"));
            assert_eq!(ran.stdout, expected, "{case} release={release}");
            assert_eq!(ran.memory.live_objects, 0, "{case} release={release}");
            assert_eq!(ran.memory.allocations, ran.memory.frees, "{case}");
        }
    }

    // tests/rt/late_effect (success)
    let rt_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/rt");
    {
        let path = rt_root.join("late_effect.fl");
        let expected = fs::read_to_string(rt_root.join("late_effect.stdout")).expect("stdout");
        let modules = run::compile(&path).expect("late_effect compiles");
        let main = run::main_of(&modules).expect("main");
        for release in [false, true] {
            let ran = run::run_main_counted(&modules, &main, 1_000_000, release)
                .unwrap_or_else(|err| panic!("late_effect release={release}: {err}"));
            assert_eq!(ran.stdout, expected, "late_effect release={release}");
            assert_eq!(ran.memory.live_objects, 0);
            assert_eq!(ran.memory.allocations, ran.memory.frees);
        }
    }

    // tests/rt/control_handlers (prints expected prefix, then ResumeTwice defect)
    {
        let path = rt_root.join("control_handlers.fl");
        let expected = fs::read_to_string(rt_root.join("control_handlers.stdout")).expect("stdout");
        let modules = run::compile(&path).expect("control_handlers compiles");
        let main = run::main_of(&modules).expect("main");
        for release in [false, true] {
            let mut heap = Heap::new();
            let report =
                run::run_main_counted_outcome(&mut heap, &modules, &main, 1_000_000, release)
                    .expect("control_handlers lowers");
            assert_eq!(
                report.stdout, expected,
                "control_handlers release={release}"
            );
            causes::assert_defect(&mut heap, report.outcome, &Error::ResumeTwice);
            let stats = heap.finish().expect("zero leaks after ResumeTwice defect");
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn tco_tail_resume_and_list_slice_run_deep_loops_in_bounded_memory() {
    let text = r"module App.BoundedMemory exposing (answer)

effect Tick =
  control tick : Int ->{} Int

sum_to : Int -> Int -> Int
let rec sum_to acc n = if n == 0 then acc else sum_to (acc + n) (n - 1)

sum_list : Int -> List Int -> Int
let rec sum_list acc xs =
  match xs with
  | [] -> acc
  | [x, ...rest] -> sum_list (acc + x) rest

tick_loop : Int -> Int ->{Tick} Int
let rec tick_loop acc i = if i == 0 then acc else tick_loop (Tick.tick acc) (i - 1)

answer : Unit ->{} Int
let answer () =
  let a = sum_to 0 20000
  let b = sum_list 0 (List.range 0 10000)
  let c =
    try tick_loop 0 10000 with
    | Tick.tick x -> resume (x + 1)
  a + b + c
";
    let entry = Name::new("App.BoundedMemory", "answer");
    for release in [false, true] {
        let program = compile_counted(text, release, &entry);
        let mut heap = Heap::new();
        let ran = execute_program(&mut heap, &program, &entry, vec![Value::Unit], 5_000_000)
            .expect("deep TCO + ListSlice + tail-resume completes");
        let expected = i32::try_from(20_000i64 * 20_001 / 2 + 9_999i64 * 10_000 / 2 + 10_000)
            .expect("fits i32");
        assert_eq!(ran.value, Value::Int(expected));
        heap.drop_value(ran.value).expect("release result");
        let stats = heap.finish().expect("zero leaks");
        assert_eq!(stats.allocations, stats.frees);
        // Peak live objects stays bounded (O(1) stack + single backing list), never O(N^2).
        assert!(
            stats.peak_live_objects < 100,
            "peak_live_objects={} should stay bounded (release={release})",
            stats.peak_live_objects
        );
    }
}

#[test]
#[ignore = "1M-10M deep recursion & deep control stress cases; full completion under --release or FLICK_COUNTED_STRESS=1"]
fn deep_recursion_and_deep_control_pass_with_zero_leaks() {
    let full = !cfg!(debug_assertions) || env::var_os("FLICK_COUNTED_STRESS").is_some();
    let run_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    for (rel, path) in [
        (
            "tests/run/codegen/recursion",
            run_root.join("codegen/recursion.fl"),
        ),
        (
            "tests/run/codegen/deep_recursion",
            run_root.join("codegen/deep_recursion.fl"),
        ),
        (
            "tests/rt/deep_control",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/rt/deep_control.fl"),
        ),
    ] {
        let expected = fs::read_to_string(path.with_extension("stdout")).expect("stdout");
        let modules = run::compile(&path).expect("compiles");
        let main = run::main_of(&modules).expect("main");
        for release in [false, true] {
            if full {
                let ran = run::run_main_counted(&modules, &main, 2_000_000_000, release)
                    .unwrap_or_else(|err| panic!("{rel} release={release}: {err}"));
                assert_eq!(ran.stdout, expected, "{rel} release={release}");
                assert_eq!(ran.memory.live_objects, 0, "{rel} release={release}");
                assert_eq!(ran.memory.allocations, ran.memory.frees, "{rel}");
                assert!(
                    ran.memory.peak_live_objects < 128,
                    "{rel} release={release}: peak_live_objects={} should stay O(1)",
                    ran.memory.peak_live_objects
                );
                eprintln!(
                    "{rel} release={release}: alloc={} free={} peak_live={}",
                    ran.memory.allocations, ran.memory.frees, ran.memory.peak_live_objects
                );
            } else {
                // Under debug `just model`, still lower and run a bounded prefix so TCO,
                // tail-resume, bounded live memory, and zero-leak OutOfFuel unwind are checked.
                let counted = run::counted_main(&modules, &main, release)
                    .unwrap_or_else(|err| panic!("{rel} release={release}: {err}"));
                let mut heap = Heap::new();
                let report = counted.execute(&mut heap, 200_000);
                assert!(
                    matches!(
                        report.outcome,
                        ProgramOutcome::Returned(Value::Unit)
                            | ProgramOutcome::Fault(Error::OutOfFuel)
                    ),
                    "{rel} release={release}: unexpected outcome {:?}",
                    report.outcome
                );
                report.outcome.release(&mut heap).expect("release outcome");
                let stats = heap.finish().expect("zero leaks on bounded prefix");
                assert_eq!(stats.live_objects, 0, "{rel} release={release}");
                assert_eq!(stats.allocations, stats.frees, "{rel} release={release}");
                assert!(
                    stats.peak_live_objects < 128,
                    "{rel} release={release}: peak_live_objects={} should stay O(1)",
                    stats.peak_live_objects
                );
            }
        }
    }
}
