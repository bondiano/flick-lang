//! Source regressions use the corpus oracle, not a second counted evaluator.
use crate::causes;
use std::{fs, path::Path};

use flick_core::{
    build,
    ir::{Lit, memory::Name},
    opt::perceus,
};
use flick_model::{
    memory::{Error, Heap, ProgramOutcome, Value, execute_program_outcome},
    run,
};

const INPUT: &str = "-123456789012345678901234567890";
const SOURCE: &str = r#"module App.BigIntArithmetic exposing (answer, negative)
answer : BigInt -> String
let answer value =
  let _unused = fun () -> value ** (-1n)
  show (value + 2n) ++ "|" ++ show (value - 2n)
    ++ "|" ++ show (value * 13n) ++ "|" ++ show (value / (-13n))
    ++ "|" ++ show (value % 13n) ++ "|" ++ show (value ** 3n)
negative : BigInt -> BigInt
let negative value = value ** (-1n)
"#;

#[test]
fn numeric_classes_release_runs_through_bigint_product_to_the_full_corpus_oracle() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/codegen/numeric_classes.fl");
    let expected = fs::read_to_string(path.with_extension("stdout")).expect("corpus oracle");
    let mut heap = Heap::new();
    let report = run::run_file_counted_outcome(&mut heap, &path, 1_000_000, true)
        .expect("checked corpus lowering");
    let outcome = format!("{:?}", report.outcome);
    if let ProgramOutcome::Returned(value) = report.outcome {
        assert_eq!(value, Value::Unit);
        heap.drop_value(value).expect("caller releases outcome");
    } else {
        let diagnostic = format!(
            "numeric_classes release must return Unit, got {outcome}; stdout={:?}",
            report.stdout
        );
        causes::unexpected(&mut heap, report.outcome, &diagnostic);
    }
    assert_eq!(report.stdout, expected);
    let stats = heap.finish().expect("all actual corpus roots released");
    assert_eq!(stats.allocations, stats.frees);
}

fn source_result(name: &str, expected: Option<&str>) {
    let modules = run::compile_source(SOURCE).expect("checked concrete BigInt source");
    let entry = Name::new("App.BigIntArithmetic", name);
    for release in [false, true] {
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
        let program = perceus::lower_with_declarations(
            ir.iter().chain(build::builtin_modules()),
            modules
                .iter()
                .map(|module| module.declarations.as_ref())
                .chain(build::builtin_declarations().iter().map(AsRef::as_ref)),
            &entry,
        )
        .expect("concrete BigInt source lowering");
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::BigInt(INPUT.into()))
            .expect("external input");
        let argument = heap.dup(&outside).expect("owned source argument");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![argument], 100_000);
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                assert_eq!(Some(heap.string(&value).expect("String outcome")), expected);
                heap.drop_value(value).expect("caller outcome release");
            }
            ProgramOutcome::Failed(cause) if expected.is_none() => {
                assert_eq!(
                    causes::defect_error(&mut heap, cause),
                    Error::BigIntNegativeExponent
                );
            }
            outcome => {
                let diagnostic = format!("{name} release={release}: {outcome:?}");
                causes::unexpected(&mut heap, outcome, &diagnostic)
            }
        }
        assert_eq!(report.stdout, "");
        assert_eq!(heap.bigint(&outside), Ok(INPUT));
        assert_eq!(heap.reference_count(&outside), Ok(1));
        heap.drop_value(outside).expect("external owner release");
        let stats = heap.finish().expect("all actual source roots released");
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn all_six_source_operators_are_unbounded_and_unused_defects_stay_cold_in_both_profiles() {
    source_result(
        "answer",
        Some(concat!(
            "-123456789012345678901234567888n|-123456789012345678901234567892n|",
            "-1604938257160493825716049382570n|9496676077872744530864197530n|0n|",
            "-1881676372353657772546716040589641726257477229849409426207693797722198701224860897069000n",
        )),
    );
}

#[test]
fn source_negative_exponents_are_specific_defects_and_release_the_owned_argument() {
    source_result("negative", None);
}
