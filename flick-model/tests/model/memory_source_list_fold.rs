//! Left folds transfer each accumulator and retain callback captures explicitly.
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

const SOURCE: &str = r#"module App.Fold exposing (answer, cold, generic, fault)
spin : Unit -> a
let rec spin () = spin ()

answer : String -> String
let answer seed =
  let initial = { text = seed }
  let folded =
    List.fold_left (fun acc item -> { acc with text = acc.text ++ item }) initial ["a", "b", "c"]
  let empty = List.fold_left (fun _ _ -> spin ()) "empty" []
  let escaped =
    List.fold_left
      (fun previous item -> fun suffix -> previous (item ++ suffix))
      (fun suffix -> seed ++ suffix)
      ["a", "b"]
  initial.text ++ "|" ++ folded.text ++ "|" ++ empty ++ "|" ++ escaped "!"

cold : String -> String
let cold seed =
  let step = List.fold_left (fun _ _ -> spin ())
  let _with_seed = step seed
  seed

generic : String -> String
let generic seed = fold (fun acc item -> acc ++ item) seed ["a", "b"]

fault : String -> Int
let fault seed =
  let prefix = seed ++ ":"
  List.fold_left
    (fun acc value ->
      if prefix == "" then acc else if value == 0 then Int.unsafe_div 1 0 else spin ()
    )
    5
    [0, 1]
"#;

fn programs(name: &str) -> (Name, [Program; 2]) {
    let modules = run::compile_source(SOURCE).expect("accepted left-fold source");
    let entry = Name::new("App.Fold", name);
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
            .expect("exact left-fold adapter lowers")
    });
    (entry, programs)
}

#[test]
fn left_fold_preserves_order_empty_aliases_and_escaping_accumulator_closures() {
    let (entry, programs) = programs("answer");
    for program in programs {
        let mut heap = Heap::new();
        let outside = heap
            .literal(&Lit::String("kept".into()))
            .expect("outside string");
        let input = heap.dup(&outside).expect("owned input alias");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], 100_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("kept|keptabc|empty|keptab!"));
        assert_eq!(report.stdout, "");
        assert_eq!(heap.string(&outside), Ok("kept"));
        heap.drop_value(value).expect("release answer");
        heap.drop_value(outside).expect("release independent alias");
        let stats = heap.finish().expect("all actual fold roots released");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn partially_applied_fold_does_not_invoke_its_callback() {
    let (entry, programs) = programs("cold");
    for program in programs {
        let mut heap = Heap::new();
        let input = heap.literal(&Lit::String("cold".into())).expect("input");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], 10_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("cold"));
        heap.drop_value(value).expect("release cold answer");
        assert_eq!(
            heap.finish().expect("partial fold released").live_objects,
            0
        );
    }
}

#[test]
fn foldable_dictionary_uses_the_independent_instances_adapter() {
    let (entry, programs) = programs("generic");
    for program in programs {
        let mut heap = Heap::new();
        let input = heap.literal(&Lit::String("kept".into())).expect("input");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], 10_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok("keptab"));
        heap.drop_value(value).expect("release generic answer");
        assert_eq!(
            heap.finish()
                .expect("fold dictionary roots released")
                .live_objects,
            0
        );
    }
}

#[test]
fn every_prefix_and_first_callback_defect_release_roots_without_sweeping_aliases() {
    for name in ["answer", "fault"] {
        let (entry, programs) = programs(name);
        for program in programs {
            let mut exhausted = false;
            let mut completed = false;
            for fuel in 0..=1200 {
                let mut heap = Heap::new();
                let outside = heap.literal(&Lit::String("kept".into())).expect("outside");
                let input = heap.dup(&outside).expect("independent input token");
                let report =
                    execute_program_outcome(&mut heap, &program, &entry, vec![input], fuel);
                match report.outcome {
                    ProgramOutcome::Returned(value) if name == "answer" => {
                        assert_eq!(heap.string(&value), Ok("kept|keptabc|empty|keptab!"));
                        completed = true;
                        heap.drop_value(value)
                            .expect("release returned fold accumulator");
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
                heap.drop_value(outside)
                    .expect("external owner releases its alias");
                let stats = heap
                    .finish()
                    .expect("no leaked accumulator/callback/element roots");
                assert_eq!(stats.live_objects, 0);
                assert_eq!(stats.allocations, stats.frees);
            }
            assert!(
                exhausted && completed,
                "both prefixes and intended outcome reached"
            );
        }
    }
}
