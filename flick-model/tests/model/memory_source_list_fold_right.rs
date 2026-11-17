//! Right folds recurse before even the callback's first pure application.
use crate::causes;
use flick_core::{
    build,
    ir::{
        Lit,
        memory::{Name, Program},
    },
    opt::perceus,
};
use flick_model::{
    memory::{Error, Heap, ProgramOutcome, execute_program_outcome},
    run,
};

const SOURCE: &str = r#"module App.FoldRight exposing (answer, cold, fault)
spin : Unit -> a
let rec spin () = spin ()

answer : String -> String
let answer seed =
  let initial = { text = seed }
  let folded =
    List.fold_right (fun item acc -> { acc with text = acc.text ++ item }) initial ["a", "b", "c"]
  let empty = List.fold_right (fun _ -> spin ()) "empty" []
  let escaped =
    List.fold_right
      (fun item previous -> fun suffix -> previous (item ++ suffix))
      (fun suffix -> seed ++ suffix)
      ["a", "b"]
  initial.text ++ "|" ++ folded.text ++ "|" ++ empty ++ "|" ++ escaped "!"

cold : String -> String
let cold seed =
  let step = List.fold_right (fun _ -> spin ())
  let _with_seed = step seed
  seed

fault : String -> Int
let fault seed =
  let prefix = seed ++ ":"
  let step value =
    let checked =
      if prefix == "" then value else if value == 0 then Int.unsafe_div 1 0 else spin ()
    fun acc -> acc + checked
  List.fold_right step 5 [1, 0]
"#;

fn programs(name: &str) -> (Name, [Program; 2]) {
    let modules = run::compile_source(SOURCE).expect("accepted right-fold source");
    let entry = Name::new("App.FoldRight", name);
    let programs = [false, true].map(|release| {
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
        perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("exact right-fold adapter lowers")
    });
    (entry, programs)
}

#[test]
fn right_fold_preserves_order_empty_aliases_and_escaping_accumulator_closures() {
    let (entry, programs) = programs("answer");
    for program in programs {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("kept".into()))
            .expect("external seed");
        let input = heap.dup(&outside).expect("owned seed alias");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], 100_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("kept|keptcba|empty|keptba!"));
        assert_eq!(report.stdout, "");
        assert_eq!(heap.string(&outside), Ok("kept"));
        heap.drop_value(value).expect("result owner");
        heap.drop_value(outside).expect("external owner");
        let stats = heap.finish().expect("fold roots explicitly released");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn partial_construction_does_not_invoke_a_divergent_callback() {
    let (entry, programs) = programs("cold");
    for program in programs {
        let mut heap = Heap::new();
        let input = heap.literal(&Lit::String("cold".into())).expect("seed");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], 10_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("cold"));
        heap.drop_value(value).expect("result owner");
        assert_eq!(
            heap.finish().expect("partial fold released").live_objects,
            0
        );
    }
}

#[test]
fn rightmost_first_stage_defect_occurs_before_left_callback_divergence() {
    let (entry, programs) = programs("fault");
    for program in programs {
        let mut heap = Heap::new();
        let input = heap.literal(&Lit::String("kept".into())).expect("seed");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], 10_000);
        causes::assert_defect(
            &mut heap,
            report.outcome,
            &Error::IntDivisionByZero { remainder: false },
        );
        assert_eq!(
            heap.finish()
                .expect("defect releases callback and accumulator roots")
                .live_objects,
            0
        );
    }
}

#[test]
fn every_fuel_prefix_through_return_or_rightmost_defect_releases_real_roots() {
    for name in ["answer", "fault"] {
        let (entry, programs) = programs(name);
        for program in programs {
            let mut exhausted = false;
            let mut completed = false;
            for fuel in 0..=1200 {
                let mut heap = Heap::new();
                let outside = heap
                    .literal(&Lit::String("kept".into()))
                    .expect("external root");
                let input = heap.dup(&outside).expect("independent input token");
                let report =
                    execute_program_outcome(&mut heap, &program, &entry, vec![input], fuel);
                match report.outcome {
                    ProgramOutcome::Returned(value) if name == "answer" => {
                        assert_eq!(heap.string(&value), Ok("kept|keptcba|empty|keptba!"));
                        completed = true;
                        heap.drop_value(value).expect("returned accumulator");
                    }
                    ProgramOutcome::Fault(Error::OutOfFuel) => exhausted = true,
                    ProgramOutcome::Failed(cause) if name == "fault" => {
                        assert_eq!(
                            causes::defect_error(&mut heap, cause),
                            Error::IntDivisionByZero { remainder: false }
                        );
                        completed = true;
                    }
                    outcome => {
                        let diagnostic =
                            format!("unexpected {name} outcome at fuel {fuel}: {outcome:?}");
                        causes::unexpected(&mut heap, outcome, &diagnostic)
                    }
                }
                assert_eq!(heap.string(&outside), Ok("kept"));
                heap.drop_value(outside).expect("external owner");
                let stats = heap
                    .finish()
                    .expect("no callback/element/accumulator roots leaked");
                assert_eq!(stats.live_objects, 0);
                assert_eq!(stats.allocations, stats.frees);
                if completed {
                    break;
                }
            }
            assert!(
                exhausted && completed,
                "every prefix and intended outcome reached"
            );
        }
    }
}
