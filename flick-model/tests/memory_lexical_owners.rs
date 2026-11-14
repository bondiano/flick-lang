//! Scope evidence survives providers, by-name calls and polymorphic containers.
#[path = "support/causes.rs"]
pub mod causes;
use std::fs;
use std::path::Path;

use flick_core::build;
use flick_core::ir::Lit;
use flick_core::ir::memory::Name;
use flick_core::opt::perceus;
use flick_model::memory::{Error, Heap, ProgramOutcome, Value, execute_main_program_outcome};
use flick_model::run;

#[test]
fn module_level_list_and_opaque_box_transport_owner_without_reconstruction() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/runtime/arena_owner_list.fl");
    let modules = run::compile(&file).expect("valid generic callback container");
    let main = run::main_of(&modules).expect("main export");
    for release in [false, true] {
        let ran = run::run_main_counted(&modules, &main, 1_000_000, release)
            .unwrap_or_else(|error| panic!("List owner release={release}: {error}"));
        assert_eq!(ran.stdout, "owned\nboxed owned\n");
        assert_eq!(ran.memory.live_objects, 0);
        assert_eq!(ran.memory.allocations, ran.memory.frees);
        assert_eq!(ran.memory.arenas_allocated, ran.memory.arenas_freed);
    }
}

#[test]
fn callback_allocations_belong_to_their_declared_live_region() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/runtime");
    let file = root.join("arena_layer_scope.fl");
    let modules = run::compile(&file).expect("valid lexical Scope acquisition");
    let main = run::main_of(&modules).expect("main export");
    let expected =
        fs::read_to_string(file.with_extension("stdout")).expect("independent expected output");
    for release in [false, true] {
        let ran = run::run_main_counted(&modules, &main, 1_000_000, release)
            .unwrap_or_else(|error| panic!("lexical owner release={release}: {error}"));
        assert_eq!(ran.stdout, expected, "release={release}");
        assert_eq!(ran.memory.live_objects, 0);
        assert_eq!(ran.memory.allocations, ran.memory.frees);
        assert_eq!(ran.memory.arenas_allocated, ran.memory.arenas_freed);
    }
}

#[test]
fn lexical_owner_adapters_release_their_captures_at_every_fuel_cut() {
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/run/runtime/arena_layer_scope.fl");
    let modules = run::compile(&file).expect("valid lexical Scope acquisition");
    let main = run::main_of(&modules).expect("main export");
    let entry = Name::new(&main.module, &main.name);
    let expected = fs::read_to_string(file.with_extension("stdout")).expect("specified stdout");

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
        let program = perceus::lower(ir.iter().chain(build::builtin_modules()), &entry)
            .expect("counted owner adapters");
        let mut completed = false;
        let mut partial_stdout = false;
        for fuel in 0..=4096 {
            let mut heap = Heap::new();
            let external = heap
                .literal(&Lit::String("outside".into()))
                .expect("independent caller root");
            let report = execute_main_program_outcome(&mut heap, &program, &entry, fuel);
            let detail = format!("release={release} fuel={fuel}: {report:?}");
            let correct = match report.outcome {
                ProgramOutcome::Returned(value) => {
                    completed = true;
                    let correct = value == Value::Unit && report.stdout == expected;
                    heap.drop_value(value).expect("release main outcome");
                    correct
                }
                ProgramOutcome::Fault(Error::OutOfFuel) => {
                    partial_stdout |= !report.stdout.is_empty() && report.stdout != expected;
                    expected.starts_with(&report.stdout)
                }
                ProgramOutcome::Raised(value) => {
                    heap.drop_value(value).expect("release unexpected failure");
                    false
                }
                ProgramOutcome::Failed(cause) => {
                    heap.drop_cause(cause).expect("unexpected cause release");
                    false
                }
                ProgramOutcome::Fault(_) => false,
            };
            assert_eq!(heap.string(&external), Ok("outside"), "{detail}");
            assert_eq!(heap.stats().live_objects, 1, "{detail}");
            heap.drop_value(external).expect("release caller root");
            let memory = heap
                .finish()
                .unwrap_or_else(|error| panic!("{detail}: {error}"));
            assert_eq!(memory.allocations, memory.frees, "{detail}");
            assert_eq!(memory.arenas_allocated, memory.arenas_freed, "{detail}");
            assert!(correct, "{detail}");
        }
        assert!(completed, "the sweep must include successful completion");
        assert!(
            partial_stdout,
            "the sweep must interrupt observable execution"
        );
    }
}
