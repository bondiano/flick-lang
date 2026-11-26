//! Independently specified graph merge/override and owner behavior.
use crate::causes;
use flick_core::{
    build,
    ir::{
        Lit,
        memory::{self, Name},
    },
    opt::perceus,
};
use flick_model::run;
use flick_vm::tier0::{Error, Heap, ProgramOutcome, execute_program_outcome};

const SOURCE: &str = r#"module App.Merge exposing (answer, uncaught)
import Arena
service Read =
  read : Unit ->{} String
service Other =
  other : Unit ->{} String
service Render =
  render : Show a => a ->{} String

type Token = Token
instance Show Token =
  let show _ = "custom"

type error Gone = Gone String

service Outer =
  outer : Unit ->{} String

outerImpl : Outer.Impl
let outerImpl = { outer = fun () -> "/outer" }

answer : String ->{} String
let answer seed =
  let left = Layer.succeed Read { read = fun () -> "wrong" }
  let right = Layer.succeed Read { read = fun () -> seed }
  let second = Layer.succeed Other { other = fun () -> "/other" }
  let renderer = Layer.succeed Render { render = fun value -> seed ++ ":" ++ show value }
  let graph = Layer.merge (Layer.merge right second) renderer
  let shared = Layer.override graph graph
  let merged = Layer.override left shared
  provide (Layer.succeed Outer outerImpl) {
    let first = Layer.run merged (fun () -> scope {
      let arena = Arena.make ()
      let text = Arena.alloc arena (Read.read () ++ Other.other () ++ Outer.outer () ++ "/" ++ Render.render Token)
      Arena.get text
    })
    first ++ "|" ++ Layer.run graph (fun () -> Read.read () ++ Other.other ())
  }
uncaught : String ->{; Gone} String
let uncaught seed =
  let layer = Layer.succeed Read { read = fun () -> seed }
  let shared = Layer.override layer layer
  Layer.run shared (fun () ->
    let text = scope {
      let arena = Arena.make ()
      let view = Arena.alloc arena (Read.read ())
      Arena.get view
    }
    from_result (Err (Gone text))
  )
"#;

fn programs(name: &str) -> (Name, [memory::Program; 2]) {
    let modules = run::compile_source(SOURCE).expect("checked graph source");
    let entry = Name::new("App.Merge", name);
    let programs = [false, true].map(|release| {
        let ir: Vec<_> = modules
            .iter()
            .map(|m| {
                if release {
                    build::optimized(&modules, m, false)
                } else {
                    (*m.ir).clone()
                }
            })
            .collect();
        perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("real counted Layer.merge lowering")
    });
    (entry, programs)
}

#[test]
fn nested_shared_override_and_outer_fallback_preserve_caller_and_run_owners() {
    let (entry, programs) = programs("answer");
    for program in programs {
        let mut success = false;
        let mut exhausted = false;
        for fuel in 0..=1800 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("seed".into()))
                .expect("allocate captured seed");
            let arg = heap.dup(&external).expect("retain caller alias");
            let report = execute_program_outcome(&mut heap, &program, &entry, vec![arg], fuel);
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    assert_eq!(
                        heap.string(&value),
                        Ok("seed/other/outer/seed:custom|seed/other"),
                        "fuel {fuel}"
                    );
                    heap.drop_value(value).expect("release owned outcome");
                    success = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => exhausted = true,
                other => {
                    let diagnostic = format!("fuel {fuel}: {other:?}");
                    causes::unexpected(&mut heap, other, &diagnostic)
                }
            }
            assert_eq!(heap.string(&external), Ok("seed"));
            assert_eq!(heap.stats().live_objects, 1, "fuel {fuel}");
            heap.drop_value(external)
                .expect("release external alias root");
            let stats = heap.finish().expect("actual heap empty without sweep");
            assert_eq!(stats.allocations, stats.frees);
            assert_eq!(stats.arenas_allocated, stats.arenas_freed);
        }
        assert!(success && exhausted);
    }
}

#[test]
fn typed_raise_through_shared_merge_closes_owner_at_every_prefix() {
    let (entry, programs) = programs("uncaught");
    for program in programs {
        let mut reached = false;
        for fuel in 0..=1000 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("seed".into()))
                .expect("allocate captured seed");
            let argument = heap.dup(&external).expect("retain caller alias");
            let report = execute_program_outcome(&mut heap, &program, &entry, vec![argument], fuel);
            match report.outcome {
                ProgramOutcome::Raised(value) => {
                    assert_eq!(heap.tag(&value), Ok("App.Merge.Gone.Gone"));
                    assert_eq!(
                        heap.string(heap.field(&value, 0).expect("owned first field")),
                        Ok("seed")
                    );
                    heap.drop_value(value).expect("release owned outcome");
                    reached = true;
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {}
                other => {
                    let diagnostic = format!("fuel {fuel}: {other:?}");
                    causes::unexpected(&mut heap, other, &diagnostic)
                }
            }
            assert_eq!(heap.string(&external), Ok("seed"));
            assert_eq!(heap.stats().live_objects, 1, "fuel {fuel}");
            heap.drop_value(external)
                .expect("release external alias root");
            let stats = heap.finish().expect("no actual objects remain");
            assert_eq!(stats.allocations, stats.frees);
            assert_eq!(stats.arenas_allocated, stats.arenas_freed);
        }
        assert!(reached);
    }
}
