//! Stage 3B-3B counted-execution regression tests and zero-leak fuel-prefix sweeps
//! for `Path`, `DateTime`, `Cron`, and `Graph`.

use std::fs;
use std::path::Path;

use flick_core::ir::Lit;
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, Value};

fn assert_fuel_prefix_zero_leaks(source: &str, expected_stdout: &str, max_steps: u64) {
    let modules = run::compile_source(source).expect("source checks");
    let main = run::main_of(&modules).expect("main export");
    let counted = run::counted_main(&modules, &main, false).expect("Stage 3B-3B counted lowering");
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
fn unlocked_stage3b_more_corpus_cases_match_stdout_with_zero_leaks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    let cases = [
        "runtime/closing_causes",
        "stdlib/effect/datetime",
        "stdlib/effect/datetime_more",
        "stdlib/effect/cron_more",
        "stdlib/effect/graph",
        "stdlib/effect/graph_algorithms",
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
fn path_datetime_cron_and_graph_json_fuel_prefix_zero_leaks() {
    let source = r#"
module Main exposing (main)

import Path
import PlatformError

main : Unit ->{Host.Console} Unit
let main () =
  let p_res =
    provide Path.layer {
      let j = Path.join ["/a", "b", "../c.fl"]
      let b = Path.basename_without ".fl" j
      let parsed = Path.parse j
      let back = Path.format parsed
      let rel = Path.relative "/a/x" "/a/c.fl"
      let inside = Path.resolve_within "/a" "c.fl"
      let bad_url =
        try Path.from_file_url "https://example.com/a" with
        | PlatformError.PlatformError _ -> "bad-scheme"
      let url =
        try Path.to_file_url "/a/c.fl" with
        | PlatformError.PlatformError _ -> "err"
      j ++ "|" ++ b ++ "|" ++ back ++ "|" ++ rel ++ "|" ++ show inside ++ "|" ++ bad_url ++ "|" ++ url
    }
  let dt_ok =
    match Json.parse "\"2024-01-01T10:00:00+02:00[Europe/Paris]\"" with
    | Ok u -> (match Codec.decode_result DateTime.json_zoned u with
      | Ok d -> (match Codec.encode_result DateTime.json_zoned d with | Ok w -> Json.stringify w | Err _ -> "e")
      | Err _ -> "e")
    | Err _ -> "e"
  let g_codec : PureCodec Json (Graph.DirectedGraph String Float) =
    Graph.json Graph.directed_kind String.json Float.json
  let g_ok =
    match Json.parse "{\"type\":\"directed\",\"nodes\":[{\"index\":0,\"data\":\"a\"},{\"index\":1,\"data\":\"b\"}],\"edges\":[{\"index\":0,\"source\":0,\"target\":1,\"data\":3}]}" with
    | Ok u -> (match Codec.decode_result g_codec u with
      | Ok g -> show (Graph.node_count g, Graph.edge_count g)
      | Err _ -> "e")
    | Err _ -> "e"
  Platform.stdout_line (p_res ++ "|" ++ dt_ok ++ "|" ++ g_ok)
"#;
    let expected = "/a/c.fl|c|/a/c.fl|../c.fl|Some \"/a/c.fl\"|bad-scheme|file:///a/c.fl|\"2024-01-01T09:00:00.000+01:00[Europe/Paris]\"|(2, 1)\n";
    assert_fuel_prefix_zero_leaks(source, expected, 25_000);
}

#[test]
fn graph_callbacks_and_cron_fuel_prefix_zero_leaks() {
    let source = r#"
module Main exposing (main)

main : Unit ->{Host.Console} Unit
let main () =
  let (_, g0) = Graph.add_node "a" (Graph.directed ())
  let (_, g1) = Graph.add_node "b" g0
  let g2 = Result.with_default g1 (Result.map (fun (_, g) -> g) (Graph.add_edge 0 1 2 g1))
  let g3 = Graph.map_nodes String.to_upper g2
  let d =
    match Graph.dijkstra { source = 0; target = 1; cost = fun w -> w } g3 with
    | Ok (Some p) -> show p.distance
    | _ -> "err"
  let c =
    match (DateTime.make "2024-03-10T08:00:00Z", Cron.parse (DateTime.zone_make_offset 0) "30 9 * * *") with
    | (Ok t, Ok cr) -> (match Cron.next t cr with | Some n -> DateTime.format_iso n | None -> "none")
    | _ -> "err"
  Platform.stdout_line (show (Graph.values (Graph.nodes g3)) ++ "|" ++ d ++ "|" ++ c)
"#;
    assert_fuel_prefix_zero_leaks(
        source,
        "[\"A\", \"B\"]|2|2024-03-10T09:30:00.000Z\n",
        25_000,
    );
}
