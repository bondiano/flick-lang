//! Fold accumulators may be cold, replayable tasks with caller-time services.
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

const SOURCE: &str = r#"module App.FoldTasks exposing (answer)
service Read =
  read : Unit ->{} String

stale : Read.Impl
let stale = { read = fun () -> show (Int.unsafe_div 1 0) }
first : Read.Impl
let first = { read = fun () -> "1" }
second : Read.Impl
let second = { read = fun () -> "2" }

left : String -> Eff {Read} none String
let left seed =
  List.fold_left
    (fun previous item -> task {
      let text = await previous
      text ++ item ++ Read.read ()
    })
    (task { seed })
    ["a", "b"]

right : String -> Eff {Read} none String
let right seed =
  List.fold_right
    (fun item previous -> task {
      let text = await previous
      text ++ item ++ Read.read ()
    })
    (task { seed })
    ["a", "b"]

answer : String ->{} String
let answer seed =
  let (from_left, from_right) =
    Layer.run (Layer.succeed Read stale) (fun () -> (left seed, right seed))
  let (l1, r1) =
    Layer.run (Layer.succeed Read first) (fun () -> (await from_left, await from_right))
  let (l2, r2) =
    Layer.run (Layer.succeed Read second) (fun () -> (await from_left, await from_right))
  l1 ++ "|" ++ l2 ++ "|" ++ r1 ++ "|" ++ r2
"#;

// Different caller services expose both outer and nested task memoization.
const EXPECTED: &str = "kepta1b1|kepta2b2|keptb1a1|keptb2a2";

fn programs() -> (Name, [Program; 2]) {
    let modules = run::compile_source(SOURCE).expect("accepted task-valued folds");
    let entry = Name::new("App.FoldTasks", "answer");
    let programs = [false, true].map(|release| {
        let ir: Vec<_> = modules
            .iter()
            .map(|module| {
                if release {
                    build::optimized(&modules, module, false)
                } else {
                    (*module.ir).clone()
                }
            })
            .collect();
        perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("tasks remain owned language accumulators")
    });
    (entry, programs)
}

#[test]
fn folded_tasks_stay_cold_replay_and_use_the_await_callers_service() {
    let (entry, programs) = programs();
    for program in programs {
        let mut heap = Heap::new();
        let input = heap.literal(&Lit::String("kept".into())).expect("seed");
        let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], 100_000);
        let ProgramOutcome::Returned(value) = report.outcome else {
            causes::unexpected(&mut heap, report.outcome, "unexpected owned outcome")
        };
        assert_eq!(heap.string(&value), Ok(EXPECTED));
        assert_eq!(report.stdout, "");
        heap.drop_value(value).expect("caller releases answer");
        let stats = heap
            .finish()
            .expect("tasks, providers and captures released");
        assert_eq!(stats.live_objects, 0);
        assert_eq!(stats.allocations, stats.frees);
    }
}

#[test]
fn every_task_fold_prefix_releases_captures_without_sweeping_external_roots() {
    let (entry, programs) = programs();
    for program in programs {
        let mut completed = false;
        let mut exhausted = false;
        for fuel in 0..=2000 {
            let mut heap = Heap::new();
            let outside = heap.literal(&Lit::String("kept".into())).expect("outside");
            let input = heap.dup(&outside).expect("independent argument token");
            let report = execute_program_outcome(&mut heap, &program, &entry, vec![input], fuel);
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(heap.string(&value), Ok(EXPECTED));
                    heap.drop_value(value).expect("release replay result");
                    completed = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => exhausted = true,
                other => {
                    let diagnostic = format!("unexpected outcome at {fuel}: {other:?}");
                    causes::unexpected(&mut heap, other, &diagnostic)
                }
            }
            assert_eq!(report.stdout, "");
            assert_eq!(heap.string(&outside), Ok("kept"));
            heap.drop_value(outside)
                .expect("external owner releases its root");
            let stats = heap
                .finish()
                .expect("every suspended task and owner released");
            assert_eq!(stats.live_objects, 0);
            assert_eq!(stats.allocations, stats.frees);
            assert_eq!(stats.arenas_allocated, stats.arenas_freed);
        }
        assert!(
            completed && exhausted,
            "prefix range includes both outcomes"
        );
    }
}
