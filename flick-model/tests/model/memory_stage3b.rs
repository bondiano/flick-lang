//! Stage 3B counted-execution regression tests and zero-leak fuel-prefix sweeps
//! for pure `packages/std` data structures, combinators, and values.

use std::fs;
use std::path::Path;

use flick_core::ir::Lit;
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value};

fn assert_fuel_prefix_zero_leaks(source: &str, expected_stdout: &str, max_steps: u64) {
    let modules = run::compile_source(source).expect("source checks");
    let main = run::main_of(&modules).expect("main export");
    let counted = run::counted_main(&modules, &main, false).expect("Stage 3B counted lowering");
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
fn unlocked_stage3b_batch1_corpus_cases_match_stdout_with_zero_leaks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "stdlib/monoids",
        "stdlib/markdown",
        "stdlib/effect/combine",
        "stdlib/effect/chunk",
        "stdlib/effect/hash_map",
        "stdlib/effect/hash_set",
        "stdlib/effect/function",
        "stdlib/effect/order",
        "stdlib/effect/trie",
        "stdlib/effect/optic_parts",
        "stdlib/effect/bytesize",
        "stdlib/effect/bytesize_text",
        "stdlib/effect/hash_ring",
        "stdlib/hash_order",
        "stdlib/effect/string",
        "stdlib/effect/number",
        "stdlib/effect/array",
        "stdlib/effect/option",
        "stdlib/effect/option_pairs",
        "stdlib/effect/record",
        "stdlib/decimals",
        "stdlib/effect/duration",
    ];
    run_corpus_cases(&root, &cases, 50_000_000);
}

#[test]
#[ignore = "heavy 1M-step Batch 3B-1 stress fixtures; run under --release -- --ignored"]
fn unlocked_stage3b_batch1_stress_corpus_cases_match_stdout_with_zero_leaks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "stdlib/collections",
        "stdlib/effect/trie_keys",
        "stdlib/effect/hash_ring_places",
    ];
    run_corpus_cases(&root, &cases, 500_000_000);
}

fn run_corpus_cases(root: &Path, cases: &[&str], fuel: u64) {
    for case in cases {
        let path = root.join(format!("{case}.fl"));
        let expected_path = root.join(format!("{case}.stdout"));
        let modules = run::compile(&path).expect("corpus compiles");
        let main = run::main_of(&modules).expect("main export");
        for release in [false, true] {
            let counted = run::counted_main(&modules, &main, release)
                .unwrap_or_else(|err| panic!("{case} (release={release}) failed to lower: {err}"));
            let mut heap = Heap::new();
            let report = counted.execute(&mut heap, fuel);
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
                .expect("release main outcome");
            let stats = heap.finish().expect("zero occupied slots");
            assert_eq!(
                stats.allocations, stats.frees,
                "{case} (release={release}) leaked: {stats:?}"
            );
            assert!(outcome_ok, "{case} (release={release}) failed: {detail}");
            let expected = fs::read_to_string(&expected_path).expect("stdout fixture");
            assert_eq!(
                stdout, expected,
                "{case} (release={release}) stdout mismatch"
            );
        }
    }
}

#[test]
fn json_patch_decimal_and_regexp_sweep_zero_leaks_at_every_fuel_prefix() {
    let source = r#"
module App.Main exposing (main)
import BigDecimal
import Formatter
import Inspectable
import Json
import JsonPointer
import RegExp
import Version

main : Unit ->{Host.Console} Unit
let main () =
  let json_out =
    match Json.parse "{\"x\":1,\"y\":[2,3]}" with
    | Ok j -> Formatter.format j ++ "~" ++ Inspectable.to_string_unknown 0 (Inspectable.to_json j)
    | Err _ -> "err"
  let dec =
    match BigDecimal.from_string "2.345" with
    | Ok d ->
      match BigDecimal.round 2 BigDecimal.HalfEven d with
      | Ok r -> BigDecimal.to_string r
      | Err _ -> "err"
    | Err _ -> "err"
  let re_out =
    match RegExp.make "(\\d+)" "g" with
    | Ok re -> RegExp.replace re "[$1]" "a1b22"
    | Err _ -> "err"
  let frag = JsonPointer.format_uri_fragment ["a/b", "c~d"]
  let parsed_frag = show (JsonPointer.parse_uri_fragment frag)
  Platform.stdout_line
    (json_out ++ "|" ++ dec ++ "|" ++ re_out ++ "|" ++ frag ++ "|" ++ parsed_frag ++ "|" ++ Version.current ())
"#;
    assert_fuel_prefix_zero_leaks(
        source,
        "{\"x\":1,\"y\":[2,3]}~{\"x\":1,\"y\":[2,3]}|2.34|a[1]b[22]|#/a~1b/c~0d|Some [\"a/b\", \"c~d\"]|4.1.2\n",
        25_000,
    );
}

#[test]
fn bytes_chunk_hashmap_and_combiner_sweep_zero_leaks_at_every_fuel_prefix() {
    let source = r#"
module App.Main exposing (main)
import Bytes
import Chunk
import Combiner
import EqHash
import HashMap
import HashSet
import Reducer

main : Unit ->{Host.Console} Unit
let main () =
  let b = Bytes.encode_utf8 "hi😀"
  let b64 = Bytes.encode_base64url b
  let c = Chunk.from_list [3, 1, 2] |> Chunk.sort
  let m = HashMap.from_seq EqHash.canonical (Seq.of_list [("b", 2), ("a", 1), ("a", 10)])
  let ms = HashMap.to_seq m
  let s = HashSet.from_seq EqHash.canonical (Seq.range 0 4)
  let prefix = Seq.to_list (Seq.take 2 (HashSet.to_seq s))
  let r = Reducer.combine_all Reducer.number_sum [1, 2, 3]
  Platform.stdout_line
    (b64 ++ "|" ++ show (Chunk.to_list c) ++ "|" ++ show (HashMap.get "a" m) ++ "|" ++ show (Seq.to_list (Seq.take 1 ms)) ++ "|" ++ show prefix ++ "|" ++ show r)
"#;
    assert_fuel_prefix_zero_leaks(
        source,
        "aGnwn5iA|[1, 2, 3]|Some 10|[(\"a\", 10)]|[0, 1]|6\n",
        25_000,
    );
}

#[test]
fn duration_exit_cause_and_effect_outcome_sweep_zero_leaks_at_every_fuel_prefix() {
    let source = r#"
module App.Main exposing (main)
import Cause
import Duration
import Effect
import Exit
import Failure
import Ref

type error Boom = Boom Int
  deriving Show

main : Unit ->{Host.Console} Unit
let main () =
  let log = Ref.make []
  let d =
    match Duration.from_input "1.5 hours" with
    | Ok v -> Duration.format v
    | Err _ -> "err"
  let ex1 = Effect.exit (task { 42 })
  let ex2 = Effect.exit (task { Fail.fail (Boom 7) })
  let ex3 = Effect.exit (task { Effect.die "boom" })
  let caught = Effect.catch_defect (fun msg -> "caught:" ++ msg) (task { Effect.die "oops" })
  let _ =
    try
      Effect.ensuring
        (task { Ref.set log (Ref.get log ++ ["fin"]) })
        (task {
          Ref.set log (Ref.get log ++ ["body"])
          Fail.fail (Boom 9)
        })
    with
    | Boom n -> n
  let rethrown =
    Effect.exit (task {
      Failure.rethrow (Cause.combine (Cause.die "d") (Cause.fail (Boom 3)))
    })
  let flags =
    match Exit.failure rethrown with
    | Some c -> show (Cause.has_fails c, Cause.has_dies c)
    | None -> "none"
  let d3 =
    match Exit.find_defect ex3 with
    | Ok s -> s
    | Err _ -> "no"
  Platform.stdout_line
    (d
      ++ "|"
      ++ show (Exit.value ex1, Exit.has_fails ex2, d3)
      ++ "|"
      ++ caught
      ++ "|"
      ++ show (Ref.get log)
      ++ "|"
      ++ flags)
"#;
    assert_fuel_prefix_zero_leaks(
        source,
        "1h 30m|(Some 42, true, \"boom\")|caught:oops|[\"body\", \"fin\"]|(true, true)\n",
        25_000,
    );
}
