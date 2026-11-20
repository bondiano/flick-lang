//! Stage 3A counted-execution regression tests and zero-leak fuel-prefix sweeps
//! for `packages/core` (`Int`, `Float`, `String`, `List`, `NonEmpty`, `Option`,
//! `Result`, `Validation`, `Record`, `Flick.Debug`) and `BigInt`.

use std::fs;
use std::path::Path;

use flick_core::ir::Lit;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value};
use flick_model::run;

fn assert_fuel_prefix_zero_leaks(source: &str, expected_stdout: &str, max_steps: u64) {
    let modules = run::compile_source(source).expect("source checks");
    let main = run::main_of(&modules).expect("main export");
    let counted = run::counted_main(&modules, &main, false).expect("Stage 3A counted lowering");
    let mut completed = false;
    for fuel in 0..=max_steps {
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("external-root".into()))
            .expect("external root");
        let report = counted.execute(&mut heap, fuel);
        let stdout = report.stdout.clone();
        match report.outcome {
            ProgramOutcome::Returned(val) => {
                heap.drop_value(val).expect("drop result");
                assert_eq!(heap.string(&external), Ok("external-root"));
                heap.drop_value(external).expect("drop external");
                let stats = heap.finish().expect("zero leaks on completion");
                assert_eq!(stats.allocations, stats.frees);
                assert_eq!(stdout, expected_stdout);
                completed = true;
                break;
            }
            ProgramOutcome::Failed(_) | ProgramOutcome::Raised(_) => {
                report.outcome.release(&mut heap).expect("drop cause");
                assert_eq!(heap.string(&external), Ok("external-root"));
                heap.drop_value(external).expect("drop external");
                let stats = heap.finish().expect("zero leaks on failure");
                assert_eq!(stats.allocations, stats.frees);
                panic!("unexpected failure at fuel={fuel}, stdout={stdout:?}");
            }
            ProgramOutcome::Fault(Error::OutOfFuel) => {
                assert_eq!(heap.string(&external), Ok("external-root"));
                heap.drop_value(external).expect("drop external");
                let stats = heap.finish().expect("zero leaks on OutOfFuel");
                assert_eq!(
                    stats.allocations, stats.frees,
                    "leak at fuel={fuel}: {stats:?}"
                );
            }
            ProgramOutcome::Fault(err) => panic!("unexpected error at fuel={fuel}: {err}"),
        }
    }
    assert!(
        completed,
        "program did not complete within {max_steps} fuel"
    );
}

#[test]
fn unlocked_stage3a_corpus_cases_match_stdout_with_zero_leaks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "codegen/strings_utf16",
        "stdlib/effect/array",
        "stdlib/effect/option",
        "stdlib/effect/option_pairs",
        "stdlib/effect/record",
        "stdlib/seqs",
        "stdlib/option_ap_strictness",
        "bootstrap/debug",
    ];
    for case in cases {
        let path = root.join(format!("{case}.fl"));
        let expected_path = root.join(format!("{case}.stdout"));
        let expected_exit_path = root.join(format!("{case}.exit"));
        let modules = run::compile(&path).expect("corpus compiles");
        let main = run::main_of(&modules).expect("main export");
        for release in [false, true] {
            let counted = run::counted_main(&modules, &main, release)
                .unwrap_or_else(|err| panic!("{case} (release={release}) failed to lower: {err}"));
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
            assert_eq!(
                stats.allocations, stats.frees,
                "{case} (release={release}) leaked: {stats:?}"
            );
            let expected = fs::read_to_string(&expected_path).expect("stdout fixture");
            assert_eq!(
                stdout, expected,
                "{case} (release={release}) stdout mismatch"
            );
            if expected_exit_path.exists() {
                assert!(
                    !outcome_ok,
                    "{case} (release={release}) unexpectedly succeeded"
                );
            } else {
                assert!(outcome_ok, "{case} (release={release}) failed: {detail}");
            }
        }
    }
}

#[test]
fn greek_lithuanian_turkish_locale_case_mapping_matches_icu() {
    let source = r#"
module App.Main exposing (main)
main : Unit ->{Host.Console} Unit
let main () =
  Platform.stdout_line (String.to_locale_upper "el" "άδικος, ή ῃ ΐ άι")
  Platform.stdout_line (String.to_locale_lower "lt" "Ì J\u{301}")
  Platform.stdout_line (show (String.length (String.to_locale_upper "lt" "i\u{307}\u{300}")))
  Platform.stdout_line (String.to_locale_lower "tur" "İI I\u{307}" ++ " " ++ String.to_locale_upper "AZ" "i")
"#;
    assert_fuel_prefix_zero_leaks(
        source,
        "ΑΔΙΚΟΣ, Ή ΗΙ Ϊ ΑΪ\ni\u{307}\u{300} j\u{307}\u{301}\n2\niı i İ\n",
        5_000,
    );
}

#[test]
fn stage3a_numeric_list_sort_and_record_sweep_zero_leaks_at_every_fuel_prefix() {
    let source = r#"
module App.Main exposing (main)
import BigInt

main : Unit ->{Host.Console} Unit
let main () =
  let a = Int.checked_add 2000000000 2000000000
  let r = Float.remainder 0.1 0.3
  let g = BigInt.gcd 48n 18n
  let xs = List.sort_by (fun (x, _) (y, _) -> compare x y) [(2, "a"), (1, "b"), (2, "c"), (1, "d")]
  let m1 = Record.from_entries [("b", 1), ("0", 2), ("a", 3)]
  let m2 = Record.map (fun v k -> show v ++ ":" ++ k) m1
  Platform.stdout_line (show a ++ "|" ++ show r ++ "|" ++ show g)
  Platform.stdout_line (show xs)
  Platform.stdout_line (show (Record.to_entries m2))
"#;
    assert_fuel_prefix_zero_leaks(
        source,
        "Err (ArithmeticError \"Int.checked_add: 4000000000 does not fit in Int\")|0|6n\n[(1, \"b\"), (1, \"d\"), (2, \"a\"), (2, \"c\")]\n[(\"0\", \"2:0\"), (\"b\", \"1:b\"), (\"a\", \"3:a\")]\n",
        20_000,
    );
}
