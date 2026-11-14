//! `App.fail` preserves its native ordinary `AppError` record payload.
use flick_core::{
    build,
    ir::{
        Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::{
    memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome},
    run,
};
use std::panic::catch_unwind;

fn program(source: &str, entry: &Name, release: bool) -> Program {
    let modules = run::compile_source(source).expect("genuine source checks");
    let ir: Vec<_> = modules
        .iter()
        .map(|m| {
            if release {
                build::optimized(&modules, m, false)
            } else {
                m.ir.clone()
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
    .expect("App.fail counted lowering")
}

#[test]
fn original_bootstrap_app_fail_raises_the_native_record_not_a_string() {
    let source = include_str!("../../tests/run/bootstrap/app_fail.fl");
    let entry = Name::new("App.Main", "main");
    // Both independent profiles are attempted before asserting RED, so the
    // baseline records semantic unsupported lowering rather than preflight.
    let results = [false, true].map(|release| catch_unwind(|| program(source, &entry, release)));
    assert!(
        results.iter().all(Result::is_ok),
        "both profiles must lower App.fail"
    );
    for result in results {
        let mut heap = Heap::new();
        let report = execute_program_outcome(
            &mut heap,
            &result.expect("both profile programs lower"),
            &entry,
            vec![Value::Unit],
            100_000,
        );
        assert_eq!(report.stdout, "before\n");
        let ProgramOutcome::Raised(payload) = report.outcome else {
            panic!(
                "expected ordinary Raised AppError, not Fault: {:?}",
                report.outcome
            )
        };
        assert_eq!(heap.tag(&payload), Ok("Prelude.AppError.AppError"));
        let record = heap
            .field(&payload, 0)
            .expect("single ordinary record payload");
        assert_eq!(
            heap.string(heap.named_field(record, "message").expect("message")),
            Ok("config \"port\" is missing")
        );
        assert_eq!(
            heap.tag(heap.named_field(record, "details").expect("details")),
            Ok("Prelude.Option.None")
        );
        heap.drop_value(payload).expect("release error");
        let stats = heap.finish().expect("actual zero occupied heap");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn checked_non_string_inputs_fail_closed() {
    for input in ["Int", "Float", "Unit", "a"] {
        let source = format!(
            "module App.CheckedFail exposing (stop)\nstop : {input} ->{{; AppError}} a\nforeign effect stop from \"flick:rt\" \"appFail\"\n"
        );
        let modules = run::compile_source(&source).expect("actual checked declaration");
        for release in [false, true] {
            let ir: Vec<_> = modules
                .iter()
                .map(|m| {
                    if release {
                        build::optimized(&modules, m, false)
                    } else {
                        m.ir.clone()
                    }
                })
                .collect();
            let error = perceus::lower_with_declarations(
                ir.iter().chain(build::builtin_modules()),
                modules
                    .iter()
                    .map(|m| m.declarations.as_ref())
                    .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
                &Name::new("App.CheckedFail", "stop"),
            )
            .expect_err("non-String input rejects");
            assert!(error.to_string().contains("App.fail"), "{error}");
        }
    }
}

#[test]
fn builtin_nonreturning_result_can_be_specialized_to_any_source_result() {
    for output in ["Unit", "Int", "String", "a"] {
        let source = format!(
            "module App.CheckedFail exposing (stop)\nstop : String ->{{; AppError}} {output}\nlet stop message = App.fail message\n"
        );
        for release in [false, true] {
            let entry = Name::new("App.CheckedFail", "stop");
            let program = program(&source, &entry, release);
            let mut heap = Heap::new();
            let message = heap
                .literal(&Lit::String("stop".into()))
                .expect("String argument");
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![message], 100_000);
            assert_eq!(report.stdout, "");
            assert!(
                matches!(&report.outcome, ProgramOutcome::Raised(_)),
                "{output}: {:?}",
                report.outcome
            );
            report
                .outcome
                .release(&mut heap)
                .expect("caller releases payload");
            let stats = heap.finish().expect("actual zero occupied heap");
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn string_alias_input_retains_external_roots_at_every_fuel_prefix() {
    let source = r#"module App.AliasFail exposing (stop)
type alias Message = String
type alias Wrapped a = a
stop : Wrapped Message ->{; AppError} a
foreign effect stop from "flick:rt" "appFail"
"#;
    let entry = Name::new("App.AliasFail", "stop");
    for release in [false, true] {
        let program = program(source, &entry, release);
        let mut terminal = false;
        for fuel in 0..1000 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("kept".into()))
                .expect("external root");
            let argument = heap.dup(&external).expect("external alias");
            let report = execute_program_outcome(&mut heap, &program, &entry, vec![argument], fuel);
            assert_eq!(report.stdout, "");
            match report.outcome {
                ProgramOutcome::Raised(payload) => {
                    assert_eq!(heap.tag(&payload), Ok("Prelude.AppError.AppError"));
                    let record = heap.field(&payload, 0).expect("ordinary payload");
                    assert_eq!(
                        heap.string(heap.named_field(record, "message").expect("message")),
                        Ok("kept")
                    );
                    assert!(
                        heap.field(&payload, 1).is_err(),
                        "exactly one physical field"
                    );
                    heap.drop_value(payload).expect("caller releases error");
                    terminal = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => {
                    let diagnostic = format!("unexpected outcome {other:?}");
                    other
                        .release(&mut heap)
                        .expect("release unexpected outcome");
                    panic!("{diagnostic}")
                }
            }
            assert_eq!(heap.string(&external), Ok("kept"));
            assert_eq!(heap.reference_count(&external), Ok(1));
            heap.drop_value(external).expect("external root release");
            let stats = heap
                .finish()
                .expect("every occupied slot released without sweeping");
            assert_eq!(stats.allocations, stats.frees);
            if terminal {
                eprintln!("App.fail release={release} first terminal fuel={fuel}");
                break;
            }
        }
        assert!(terminal, "consecutive prefixes reach a typed failure");
    }
}

#[test]
fn partial_application_is_cold_and_try_binds_the_native_record_payload() {
    let source = r#"module App.ColdFail exposing (cold, caught, callback_caught)
cold : Unit -> (String ->{; AppError} Int)
let cold () = App.fail
caught : Unit ->{} String
let caught () =
  try App.fail "caught" with
  | AppError payload -> payload.message
invoke : (String ->{; AppError} String) ->{; AppError} String
let invoke callback = callback "callback"
callback_caught : Unit ->{} String
let callback_caught () =
  try invoke App.fail with
  | AppError payload -> payload.message
"#;
    for release in [false, true] {
        for (name, expected) in [
            ("cold", None),
            ("caught", Some("caught")),
            ("callback_caught", Some("callback")),
        ] {
            let entry = Name::new("App.ColdFail", name);
            let program = program(source, &entry, release);
            let mut heap = Heap::new();
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
            assert_eq!(report.stdout, "");
            let ProgramOutcome::Returned(value) = report.outcome else {
                panic!("cold/caught never raises: {:?}", report.outcome)
            };
            if let Some(expected) = expected {
                assert_eq!(heap.string(&value), Ok(expected));
            }
            heap.drop_value(value).expect("release result");
            let stats = heap.finish().expect("no occupied heap slots");
            assert_eq!(stats.allocations, stats.frees);
        }
    }
}

#[test]
fn a_user_app_module_keeps_its_own_fail_binding() {
    let source = r#"module App exposing (main, fail)
fail : String ->{} Unit
let fail message = Platform.stdout_line ("user " ++ message)
main : Unit ->{} Unit
let main () = fail "binding"
"#;
    let modules = run::compile_source(source).expect("user App module checks");
    let main = run::main_of(&modules).expect("main export");
    for release in [false, true] {
        let mut heap = Heap::new();
        let report = run::run_main_counted_outcome(&mut heap, &modules, &main, 100_000, release)
            .expect("user binding takes precedence over builtin fallback");
        assert_eq!(report.stdout, "user binding\n");
        assert!(
            matches!(&report.outcome, ProgramOutcome::Returned(Value::Unit)),
            "{:?}",
            report.outcome
        );
        report.outcome.release(&mut heap).expect("release result");
        let stats = heap.finish().expect("no occupied heap slots");
        assert_eq!(stats.allocations, stats.frees);
    }
}
