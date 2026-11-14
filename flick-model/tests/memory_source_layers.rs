//! Independent source expectations for counted constant Layer provision.
#[path = "support/causes.rs"]
pub mod causes;
use std::{fs, path::Path};

use flick_core::build;
use flick_core::ir::Lit;
use flick_core::ir::memory::{self, Name};
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, execute_program_outcome};
use flick_model::run;

#[test]
fn constant_layer_fixtures_execute_in_both_profiles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run");
    for case in [
        "codegen/service_place",
        "codegen/service_dictionary",
        "codegen/effect_sets",
        "effect/row_without",
    ] {
        let modules = run::compile(&root.join(format!("{case}.fl"))).expect("source checks");
        let main = run::main_of(&modules).expect("main export");
        let expected =
            fs::read_to_string(root.join(format!("{case}.stdout"))).expect("specified stdout");
        for release in [false, true] {
            let ran = run::run_main_counted(&modules, &main, 1_000_000, release)
                .unwrap_or_else(|error| panic!("{case} release={release}: {error}"));
            assert_eq!(ran.stdout, expected, "{case} release={release}");
            assert_eq!(ran.memory.live_objects, 0);
            assert_eq!(ran.memory.allocations, ran.memory.frees);
        }
    }
}

const DISPATCH: &str = r#"module App.LayerDispatch exposing (answer)

service Render =
  render : (Show a, Show b) => a -> b ->{} String
  other : Unit ->{} String

old : Render.Impl
let old = { render = fun a b -> "old"; other = fun () -> "old" }

answer : String ->{} String
let answer seed =
  let partial = provide (Layer.succeed Render old) { Render.render 7 }
  let delayed = provide (Layer.succeed Render old) { task { Render.render true 8 } }
  let closure = provide (Layer.succeed Render old) { fun () -> Render.render false 9 }
  let current = Layer.succeed Render {
    render = fun first second -> seed ++ ":" ++ show first ++ "/" ++ show second;
    other = fun () -> "other";
  }
  Layer.run current (fun unit ->
    ignore unit
    try partial false ++ "|" ++ await delayed ++ "|" ++ closure () with
    | Render.other () -> "wrong inner other"
  )
"#;

const CALLBACKS: &str = r#"module App.LayerCallbacks exposing (answer, uncaught)
import Arena

service Read =
  read : Unit ->{} String

effect Add =
  add : String ->{} String

type error Gone = Gone String

uncaught : String ->{; Gone} String
let uncaught seed =
  let layer = Layer.succeed Read { read = fun () -> seed }
  Layer.run layer (fun unit ->
    ignore unit
    Platform.stdout_line "before"
    let value = scope {
      let arena = Arena.make ()
      let view = Arena.alloc arena (Read.read ())
      let cell = Ref.make (Arena.get view)
      Ref.get cell
    }
    Platform.stdout_line "inside"
    from_result (Err (Gone value))
  )

answer : String ->{} String
let answer seed =
  let layer = Layer.succeed Read { read = fun () -> seed }
  let callback = fun unit ->
    ignore unit
    let suffix = "/callback"
    if seed == "seed" then
      try Add.add (Read.read () ++ suffix) with
      | Add.add text -> text ++ "/handler"
    else "wrong branch"
  let first = Layer.run layer callback
  let again = Layer.run layer (fun () -> Read.read ())
  let caught =
    try uncaught seed with
    | Gone value -> "caught:" ++ value
  first ++ "|" ++ again ++ "|" ++ caught
"#;

fn programs(text: &str, module: &str, name: &str) -> (Name, [memory::Program; 2]) {
    let modules = run::compile_source(text).expect("source checks");
    let entry = Name::new(module, name);
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
        perceus::lower(ir.iter().chain(build::builtin_modules()), &entry).expect("counted lowering")
    });
    (entry, programs)
}

fn release_outcome(heap: &mut Heap, outcome: ProgramOutcome, expected: &str, raised: bool) -> bool {
    match outcome {
        ProgramOutcome::Returned(value) => {
            let correct = !raised && heap.string(&value) == Ok(expected);
            heap.drop_value(value)
                .expect("caller releases returned token");
            correct
        }
        ProgramOutcome::Raised(value) => {
            let correct = raised
                && heap.tag(&value) == Ok("App.LayerCallbacks.Gone.Gone")
                && heap
                    .field(&value, 0)
                    .is_ok_and(|text| heap.string(text) == Ok(expected));
            heap.drop_value(value)
                .expect("caller releases raised token");
            correct
        }
        ProgramOutcome::Failed(cause) => {
            heap.drop_cause(cause).expect("unexpected cause release");
            false
        }
        ProgramOutcome::Fault(_) => false,
    }
}

fn check_runs(
    text: &str,
    module: &str,
    name: &str,
    expected: &str,
    stdout: &str,
    raised: bool,
    cuts: bool,
) {
    let (entry, programs) = programs(text, module, name);
    for program in programs {
        let mut completed = 0;
        let mut exhausted = 0;
        let mut partial_stdout = false;
        let fuels = if cuts { 0..=2048 } else { 100_000..=100_000 };
        for fuel in fuels {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("seed".into()))
                .expect("external root");
            let argument = heap.dup(&external).expect("independent alias");
            let report = execute_program_outcome(&mut heap, &program, &entry, vec![argument], fuel);
            let detail = format!("fuel {fuel}: {report:?}");
            let prefix = stdout.starts_with(&report.stdout);
            let correct = match report.outcome {
                ProgramOutcome::Fault(Error::OutOfFuel) if cuts => {
                    exhausted += 1;
                    partial_stdout |= !report.stdout.is_empty() && report.stdout != stdout;
                    true
                }
                outcome => {
                    completed += 1;
                    release_outcome(&mut heap, outcome, expected, raised) && report.stdout == stdout
                }
            };
            assert_eq!(heap.string(&external), Ok("seed"), "{detail}");
            assert_eq!(
                heap.stats().live_objects,
                1,
                "only caller alias survives: {detail}"
            );
            heap.drop_value(external).expect("external root");
            let memory = heap
                .finish()
                .unwrap_or_else(|error| panic!("{detail}: {error}"));
            assert_eq!(memory.allocations, memory.frees, "{detail}");
            assert!(prefix && correct, "{detail}");
        }
        assert!(completed > 0, "successful completion must be reached");
        if cuts {
            assert!(exhausted > 0);
            if !stdout.is_empty() {
                assert!(partial_stdout);
            }
        }
    }
}

#[test]
fn two_dictionary_staging_and_escaped_partial_tasks_closures_use_current_provider() {
    check_runs(
        DISPATCH,
        "App.LayerDispatch",
        "answer",
        "seed:7/false|seed:true/8|seed:false/9",
        "",
        false,
        true,
    );
}

#[test]
fn arbitrary_callback_locals_branches_handlers_refs_scopes_catches_and_replay() {
    check_runs(
        CALLBACKS,
        "App.LayerCallbacks",
        "answer",
        "seed/callback/handler|seed|caught:seed",
        "before\ninside\n",
        false,
        true,
    );
}

#[test]
fn callback_failure_transfers_payload_and_closes_owner_at_every_fuel_cut() {
    check_runs(
        CALLBACKS,
        "App.LayerCallbacks",
        "uncaught",
        "seed",
        "before\ninside\n",
        true,
        true,
    );
}

#[test]
fn unknown_layer_adapters_reject_before_execution_or_heap_changes() {
    use flick_core::ir::ItemKind;
    use flick_core::opt::perceus::LowerError;

    for binding in ["layerSucceed", "layerRun"] {
        let module = build::builtin_modules()
            .iter()
            .find(|module| {
                module.items.iter().any(|item| {
                    matches!(&item.kind,
                ItemKind::Foreign(value) if value.binding.name == binding)
                })
            })
            .expect("live builtin");
        let mut changed = module.clone();
        let (span, name) = changed
            .items
            .iter_mut()
            .find_map(|item| match &mut item.kind {
                ItemKind::Foreign(value) if value.binding.name == binding => {
                    value.adapter.cancel = true;
                    Some((item.span, value.name.clone()))
                }
                _ => None,
            })
            .expect("changed builtin");
        let mut heap = Heap::new();
        let external = heap
            .literal(&Lit::String("outside".into()))
            .expect("external root");
        let before = heap.stats();
        let result = perceus::lower([&changed], &Name::new(&changed.name, &name));
        assert!(
            matches!(result, Err(LowerError::Unsupported { span: actual, .. }) if actual == span)
        );
        assert_eq!(heap.stats(), before, "lowering must precede execution");
        assert_eq!(heap.string(&external), Ok("outside"));
        heap.drop_value(external).expect("external root");
        assert_eq!(heap.finish().expect("no execution roots").live_objects, 0);
    }
}

#[test]
fn malformed_complete_provider_is_a_shadow_barrier_not_a_partial_handler() {
    use flick_core::ir::{ExprKind, ItemKind};
    use flick_model::memory::Value;

    let text = r#"module App.LayerBarrier exposing (answer)

service Read =
  read : Unit ->{} String

broken : Read.Impl
let broken = { read = fun () -> "inner" }

outer : Read.Impl
let outer = { read = fun () -> "wrong outer fallback" }

answer : Unit ->{} String
let answer () = provide Read outer {
  provide (Layer.succeed Read broken) { Read.read () }
}
"#;
    for nonrecord in [false, true] {
        // Deliberately malformed checked IR exercises the source bridge's
        // complete-provider boundary. Well-typed source requires every method.
        let mut modules = run::compile_source(text).expect("well-typed source before corruption");
        let body = modules
            .iter_mut()
            .flat_map(|module| &mut module.ir.items)
            .find_map(|item| match &mut item.kind {
                ItemKind::Value { name, body, .. } if name == "broken" => Some(body),
                _ => None,
            })
            .expect("implementation definition");
        body.kind = if nonrecord {
            ExprKind::Lit(Lit::Unit)
        } else {
            ExprKind::Record(vec![])
        };
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
            let entry = Name::new("App.LayerBarrier", "answer");
            let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
                .expect("lower malformed provider payload");
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("outside".into()))
                .expect("external alias");
            let report =
                execute_program_outcome(&mut heap, &program, &entry, vec![Value::Unit], 100_000);
            let expected = if nonrecord {
                Error::ExpectedRecord
            } else {
                Error::MissingNamedField("read".into())
            };
            let correct =
                matches!(report.outcome, ProgramOutcome::Fault(ref error) if *error == expected);
            report
                .outcome
                .release(&mut heap)
                .expect("release every unexpected owned outcome");
            assert_eq!(heap.string(&external), Ok("outside"));
            heap.drop_value(external).expect("external alias");
            assert_eq!(
                heap.finish()
                    .expect("fault drains provider roots")
                    .live_objects,
                0
            );
            assert!(
                correct,
                "malformed provider must not reach the outer handler"
            );
            assert_eq!(report.stdout, "");
        }
    }
}
