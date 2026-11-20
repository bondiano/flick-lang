//! Stage 3C counted-execution regression tests and zero-leak fuel-prefix sweeps
//! for `Schema`, `Codec`, `Check`, `JsonSchema`, `Route`, `Differ.json_patch`,
//! `Optic`, and `Open`.

use std::fs;
use std::path::Path;

use flick_core::ir::Lit;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value};
use flick_model::run;

fn assert_fuel_prefix_zero_leaks(source: &str, expected_stdout: &str, max_steps: u64) {
    let modules = run::compile_source(source).expect("source checks");
    let main = run::main_of(&modules).expect("main export");
    let counted = run::counted_main(&modules, &main, false).expect("Stage 3C counted lowering");
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
fn unlocked_stage3c_corpus_cases_match_stdout_with_zero_leaks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "schema/derive",
        "schema/json_wire",
        "schema/enums",
        "schema/combinators",
        "schema/route_nested",
        "stdlib/effect/schema",
        "stdlib/effect/codec",
        "stdlib/effect/check",
        "stdlib/effect/json_schema",
        "stdlib/effect/duration_text",
        "stdlib/effect/values",
    ];
    for case in cases {
        let path = root.join(format!("{case}.fl"));
        let expected_path = root.join(format!("{case}.stdout"));
        let modules = run::compile(&path).expect("corpus compiles");
        let main = run::main_of(&modules).expect("main export");
        for release in [false, true] {
            let counted = run::counted_main(&modules, &main, release)
                .unwrap_or_else(|err| panic!("{case} (release={release}) failed to lower: {err}"));
            let mut heap = Heap::new();
            let report = counted.execute(&mut heap, 50_000_000);
            let outcome_ok = matches!(report.outcome, ProgramOutcome::Returned(Value::Unit));
            let stdout = report.stdout.clone();
            let detail = match &report.outcome {
                ProgramOutcome::Failed(cause) => {
                    let reasons: Option<Vec<_>> =
                        heap.cause_reasons(cause).ok().map(Iterator::collect);
                    format!("Failed({reasons:?}) stdout={stdout:?}")
                }
                other => format!("{other:?} stdout={stdout:?}"),
            };
            report
                .outcome
                .release(&mut heap)
                .expect("release program outcome");
            let stats = heap
                .finish()
                .unwrap_or_else(|err| panic!("{case} (release={release}) heap leak: {err}"));
            assert_eq!(
                stats.allocations, stats.frees,
                "{case} (release={release}) leak: {stats:?}"
            );
            assert!(outcome_ok, "{case} (release={release}) outcome: {detail}");
            let expected = fs::read_to_string(&expected_path).expect("expected stdout");
            assert_eq!(
                stdout, expected,
                "{case} (release={release}) stdout mismatch"
            );
        }
    }
}

#[test]
fn schema_encode_decode_and_json_schema_fuel_prefix_zero_leaks() {
    let source = r#"
module Main exposing (main)

type Expr = Num Float | Neg Expr
  deriving Eq, Show, Schema

main : Unit ->{Host.Console} Unit
let main () =
  let c = Codec.json Expr.schema
  let wire = match Codec.encode_result c (Neg (Num 4)) with | Ok w -> Json.stringify w | Err _ -> "err"
  let js = match Codec.json_schema "Expr" c with | Ok s -> String.contains "$defs" s | Err _ -> false
  Platform.stdout_line (wire ++ "|" ++ show js)
"#;
    assert_fuel_prefix_zero_leaks(
        source,
        "{\"_tag\":\"Neg\",\"_0\":{\"_tag\":\"Num\",\"_0\":4}}|true\n",
        25_000,
    );
}

#[test]
fn check_and_route_fuel_prefix_zero_leaks() {
    let source = r#"
module Main exposing (main)

type Page = Home | User Float
  deriving Eq, Show, Schema, Url

main : Unit ->{Host.Console} Unit
let main () =
  let chk = Check.apply (Check.min_length 2) String.json
  let ok_chk = match Json.parse "\"hi\"" with | Ok u -> (match Codec.decode_result chk u with | Ok s -> s | Err _ -> "bad") | Err _ -> "bad"
  let url = Route.print Page.url (User 7)
  let parsed = Route.parse Page.url url == Some (User 7)
  Platform.stdout_line (ok_chk ++ "|" ++ url ++ "|" ++ show parsed)
"#;
    assert_fuel_prefix_zero_leaks(source, "hi|/user/7|true\n", 25_000);
}
