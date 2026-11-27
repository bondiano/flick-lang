//! Differential save-then-load property tests and end-to-end source tests for
//! per-module VM images (`#988`).
//!
//! 1. Generated well-typed programs from `flick_model::generate` saved mid-run
//!    (and cold) into `.fvm` bytes and restored into a fresh VM produce the
//!    exact same outcome, stdout, and structural value as an uninterrupted
//!    tier-0 run, with zero leaked heap slots on both VMs.
//! 2. Real Flick source programs verify DAG aliasing, `Ref` cycles, closures,
//!    partial applications, per-module linking with interface checks, and
//!    `unload_module` releasing all live slots after the last closure drops.

use std::collections::BTreeSet;
use std::io;
use std::path::{Path, PathBuf};
use std::slice;

use flick_core::build::{builtin_bundle_of, builtin_declarations};
use flick_core::diagnostics::Severity;
use flick_core::iface::{self, Interface};
use flick_core::ir::Module as IrModule;
use flick_core::ir::memory::{Name, Program};
use flick_core::opt::perceus::lower_roots_with_declarations;
use flick_core::project::{self, Host};
use flick_core::source::SourceMap;
use flick_model::generate;
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_model::types::Type;
use flick_vm::image::{
    SaveSelection, VmImage, build_static_image, execute_image_main_with_args, execute_image_program,
};
use flick_vm::session::{EvalOutcome, VmSession};
use flick_vm::stdlib;
use flick_vm::tier0::{
    Error as MemoryError, ExecutionReport, Heap, HostMode, ProgramOutcome, Value,
    execute_main_program_outcome_with_host as tier0_exec_main,
    execute_program_outcome as tier0_exec_program,
};

fn values_structurally_equal(h0: &Heap, v0: &Value, h1: &Heap, v1: &Value) -> Result<bool, String> {
    match (v0, v1) {
        (Value::Unit, Value::Unit) => Ok(true),
        (Value::Bool(a), Value::Bool(b)) => Ok(a == b),
        (Value::Int(a), Value::Int(b)) => Ok(a == b),
        (Value::Int8(a), Value::Int8(b)) => Ok(a == b),
        (Value::Int16(a), Value::Int16(b)) => Ok(a == b),
        (Value::Int64(a), Value::Int64(b)) => Ok(a == b),
        (Value::UInt8(a), Value::UInt8(b)) => Ok(a == b),
        (Value::UInt16(a), Value::UInt16(b)) => Ok(a == b),
        (Value::UInt32(a), Value::UInt32(b)) => Ok(a == b),
        (Value::UInt64(a), Value::UInt64(b)) => Ok(a == b),
        (Value::Float(a), Value::Float(b)) => {
            Ok(a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()))
        }
        (Value::Float32(a), Value::Float32(b)) => {
            Ok(a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan()))
        }
        (
            Value::Aggregate {
                tag: t0,
                ordinary_fields: o0,
                fields: f0,
            },
            Value::Aggregate {
                tag: t1,
                ordinary_fields: o1,
                fields: f1,
            },
        ) => {
            if t0 != t1 || o0 != o1 {
                return Ok(false);
            }
            for (a, b) in f0[..*o0].iter().zip(&f1[..*o1]) {
                if !values_structurally_equal(h0, a, h1, b)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (Value::Object(_), Value::Object(_)) => {
            if let (Ok(s0), Ok(s1)) = (h0.string(v0), h1.string(v1)) {
                return Ok(s0 == s1);
            }
            if let (Ok(b0), Ok(b1)) = (h0.bigint(v0), h1.bigint(v1)) {
                return Ok(b0 == b1);
            }
            if let (Ok(t0), Ok(t1)) = (h0.tag(v0), h1.tag(v1)) {
                if t0 != t1 {
                    return Ok(false);
                }
                let mut i = 0;
                loop {
                    match (h0.field(v0, i), h1.field(v1, i)) {
                        (Ok(a), Ok(b)) => {
                            if !values_structurally_equal(h0, a, h1, b)? {
                                return Ok(false);
                            }
                            i += 1;
                        }
                        (Err(_), Err(_)) => return Ok(true),
                        _ => return Ok(false),
                    }
                }
            }
            match (stdlib::show_value(h0, v0), stdlib::show_value(h1, v1)) {
                (Ok(s0), Ok(s1)) => Ok(s0 == s1),
                (Err(_), Err(_)) => Ok(true),
                (r0, r1) => Err(format!("show mismatch: {r0:?} vs {r1:?}")),
            }
        }
        _ => Ok(false),
    }
}

fn assert_outcomes_match(
    seed: usize,
    label: &str,
    source: &str,
    h0: &Heap,
    r0: &ExecutionReport,
    h1: &Heap,
    r1: &ExecutionReport,
) {
    assert_eq!(
        r0.stdout, r1.stdout,
        "seed {seed} ({label}) stdout mismatch\n{source}"
    );
    match (&r0.outcome, &r1.outcome) {
        (ProgramOutcome::Returned(v0), ProgramOutcome::Returned(v1))
        | (ProgramOutcome::Raised(v0), ProgramOutcome::Raised(v1)) => {
            let eq = values_structurally_equal(h0, v0, h1, v1)
                .unwrap_or_else(|e| panic!("seed {seed} ({label}) compare error: {e}\n{source}"));
            assert!(
                eq,
                "seed {seed} ({label}) value mismatch:\nbase={v0:?}\nreloaded={v1:?}\n{source}"
            );
        }
        (ProgramOutcome::Failed(_), ProgramOutcome::Failed(_))
        | (
            ProgramOutcome::Fault(MemoryError::OutOfFuel),
            ProgramOutcome::Fault(MemoryError::OutOfFuel),
        ) => {}
        (o0, o1) => {
            panic!(
                "seed {seed} ({label}) outcome mismatch:\nbase={o0:?}\nreloaded={o1:?}\n{source}"
            );
        }
    }
}

fn check_warm_roundtrip(
    counted: &run::CountedMain,
    interfaces: &[Interface],
    seed: usize,
    label: &str,
    source: &str,
    fuel: u64,
    baseline: (&Heap, &ExecutionReport),
) {
    let (heap0, r0) = baseline;
    let mut s1 = VmSession::with_host_mode(HostMode::Deterministic);
    for iface in interfaces {
        s1.register_module_interface(iface.clone());
    }
    let init_outcome = s1.eval_declarations(
        &counted.program,
        &counted.entry.module,
        slice::from_ref(&counted.entry),
        counted.root.as_ref(),
        None,
    );
    if matches!(init_outcome, EvalOutcome::Declaration { .. }) {
        let warm_img = s1
            .save_image(Some(counted.entry.clone()))
            .unwrap_or_else(|e| panic!("seed {seed} warm save failed: {e}\n{source}"));
        let warm_bytes = warm_img.encode().expect("encode warm");
        s1.shutdown().expect("zero leaks VM 1");

        let warm_decoded = VmImage::decode(&warm_bytes).expect("decode warm");
        assert!(warm_decoded.has_initialized_globals());
        let mut heap_warm = Heap::new();
        let r_warm = execute_image_main_with_args(
            &mut heap_warm,
            &warm_decoded,
            &counted.entry,
            fuel,
            HostMode::Deterministic,
            vec![],
        )
        .expect("exec warm main");
        assert_outcomes_match(seed, label, source, heap0, r0, &heap_warm, &r_warm);
        r_warm
            .outcome
            .release(&mut heap_warm)
            .expect("release warm");
        let warm_stats = heap_warm.finish().expect("zero leaks VM 2");
        assert_eq!(warm_stats.live_objects, 0);
    } else {
        s1.shutdown().expect("zero leaks VM 1 after init fail");
    }
}

fn check_generated_roundtrip(
    counted: &run::CountedMain,
    interfaces: &[Interface],
    is_unit_fn: bool,
    seed: usize,
    release: bool,
    source: &str,
    fuel: u64,
) {
    let mut heap0 = Heap::new();
    let r0 = if is_unit_fn {
        tier0_exec_main(
            &mut heap0,
            &counted.program,
            &counted.entry,
            fuel,
            HostMode::Deterministic,
        )
    } else {
        tier0_exec_program(&mut heap0, &counted.program, &counted.entry, vec![], fuel)
    };

    let cold_img = build_static_image(
        counted.program.clone(),
        interfaces.to_vec(),
        Some(counted.entry.clone()),
        counted.root.clone(),
    )
    .unwrap_or_else(|e| panic!("seed {seed} cold build failed: {e}"));
    let cold_bytes = cold_img.encode().expect("encode cold");
    let cold_decoded = VmImage::decode(&cold_bytes).expect("decode cold");
    let mut heap_cold = Heap::new();
    let r_cold = if is_unit_fn {
        execute_image_main_with_args(
            &mut heap_cold,
            &cold_decoded,
            &counted.entry,
            fuel,
            HostMode::Deterministic,
            vec![],
        )
        .expect("exec cold main")
    } else {
        execute_image_program(
            &mut heap_cold,
            &cold_decoded,
            &counted.entry,
            vec![],
            fuel,
            HostMode::Deterministic,
        )
        .expect("exec cold prog")
    };
    assert_outcomes_match(
        seed,
        if release {
            "cold/release"
        } else {
            "cold/debug"
        },
        source,
        &heap0,
        &r0,
        &heap_cold,
        &r_cold,
    );
    r_cold
        .outcome
        .release(&mut heap_cold)
        .expect("release cold");
    let cold_stats = heap_cold.finish().expect("zero leaks cold");
    assert_eq!(cold_stats.live_objects, 0);

    if is_unit_fn {
        let label = if release {
            "warm/release"
        } else {
            "warm/debug"
        };
        check_warm_roundtrip(
            counted,
            interfaces,
            seed,
            label,
            source,
            fuel,
            (&heap0, &r0),
        );
    }

    r0.outcome.release(&mut heap0).expect("release baseline");
    heap0.finish().expect("zero leaks baseline");
}

#[test]
fn generated_programs_save_and_load_match_uninterrupted_tier0_with_zero_leaks() {
    let mut compared = 0usize;
    let mut warm_compared = 0usize;
    let mut rng_state: u64 = 0x9880_beef_4321_8765;
    let mut next_u32 = || {
        rng_state = rng_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from(rng_state >> 32).expect("shifted u64 fits u32")
    };
    let fuel = 100_000;
    let entry_path = ModelPath::new("Fixture", "program");

    for seed in 0..350 {
        let len = (next_u32() as usize % 36) + 6;
        let choices: Vec<u32> = (0..len).map(|_| next_u32()).collect();
        let p = generate::program(&choices);
        if !generate::runs(&p) || matches!(&p.scheme.body, Type::Eff(..)) {
            continue;
        }
        let is_unit_fn = matches!(
            &p.scheme.body,
            Type::Fun(u, _, _) if **u == Type::con("Prelude", "Unit")
        );
        let source = generate::source(&p);
        let Ok(modules) = run::compile_source(&source) else {
            continue;
        };
        let interfaces: Vec<Interface> = modules.iter().map(|m| m.interface.clone()).collect();
        for release in [false, true] {
            let Ok(counted) = run::counted_main(&modules, &entry_path, release) else {
                continue;
            };
            check_generated_roundtrip(
                &counted,
                &interfaces,
                is_unit_fn,
                seed,
                release,
                &source,
                fuel,
            );
            compared += 1;
            if is_unit_fn {
                warm_compared += 1;
            }
        }
    }

    assert!(
        compared >= 60,
        "expected at least 60 generated program save/load roundtrips, got {compared}"
    );
    assert!(
        warm_compared >= 10,
        "expected at least 10 mid-run warm snapshot roundtrips, got {warm_compared}"
    );
}

struct MemHost<'a>(&'a [(&'a str, &'a str)]);

impl Host for MemHost<'_> {
    fn read(&self, p: &Path) -> io::Result<Vec<u8>> {
        let s = p.to_string_lossy();
        self.0
            .iter()
            .find(|(path, _)| *path == s)
            .map(|(_, content)| content.as_bytes().to_vec())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn display(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

fn lower_source_files(files: &[(&str, &str)], entries: &[Name]) -> (Vec<Interface>, Program) {
    let mut sources = SourceMap::default();
    let roots: Vec<PathBuf> = files.iter().map(|(p, _)| PathBuf::from(p)).collect();
    let (diagnostics, compilation) = project::compile(&MemHost(files), &mut sources, None, &roots);
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        errors.is_empty(),
        "compile Flick source: {}",
        errors.join("\n")
    );
    let modules = compilation.modules;
    let interfaces: Vec<Interface> = modules.iter().map(|m| m.interface.clone()).collect();
    let mut explicit_ir = Vec::new();
    let mut decls = Vec::new();
    for m in &modules {
        explicit_ir.push((*m.ir).clone());
        decls.push(m.declarations.as_ref());
    }
    let builtin_decls = builtin_declarations();
    let mut all_decls: Vec<&Interface> = decls;
    let seen: BTreeSet<&str> = all_decls.iter().map(|d| d.module.as_str()).collect();
    for b in builtin_decls {
        if !seen.contains(b.module.as_str()) {
            all_decls.push(b.as_ref());
        }
    }
    let builtin_bundle = builtin_bundle_of(false);
    let mut all_ir: Vec<&IrModule> = explicit_ir.iter().collect();
    for b in &builtin_bundle.modules {
        all_ir.push(b);
    }
    let root_refs: Vec<&Name> = entries.iter().collect();
    let program = lower_roots_with_declarations(all_ir, all_decls, &root_refs)
        .expect("lower source to memory IR");
    (interfaces, program)
}

fn verify_loaded_counter_session(s2: &mut VmSession, source: &str, expected_iface: &Interface) {
    let driver_source = "\
module Driver2 exposing (check, pair_val)

import Counter

pair_val : (String, String)
let pair_val = Counter.pair

check : Unit ->{} (Int, Int, Int, Int)
let check () =
  let (step, read) = Counter.make_counter ()
  let a = step ()
  let b = step ()
  let c = read ()
  (a, b, c, Counter.add10 32)
";
    let driver_entries = [
        Name::new("Driver2", "pair_val"),
        Name::new("Driver2", "check"),
    ];
    let (_, call_rest) = lower_source_files(
        &[("/Counter.fl", source), ("/Driver2.fl", driver_source)],
        &driver_entries,
    );
    let pair_out = s2.eval_phrase(
        &call_rest,
        &Name::new("Driver2", "pair_val"),
        None,
        false,
        None,
    );
    assert_eq!(
        pair_out,
        EvalOutcome::Value {
            rendered: "(\"hello-shared\", \"hello-shared\")".into(),
            stdout: String::new(),
        }
    );
    let decl_out = s2.eval_declarations(
        &call_rest,
        "Driver2",
        &[Name::new("Driver2", "check")],
        None,
        None,
    );
    assert_eq!(
        decl_out,
        EvalOutcome::Declaration {
            stdout: String::new()
        }
    );

    let combined_bytes = s2
        .save_image(Some(Name::new("Driver2", "check")))
        .expect("save combined")
        .encode()
        .expect("encode combined");
    let combined = VmImage::decode(&combined_bytes).expect("decode combined");
    let mut h3 = Heap::new();
    let r3 = execute_image_main_with_args(
        &mut h3,
        &combined,
        &Name::new("Driver2", "check"),
        100_000,
        HostMode::Deterministic,
        vec![],
    )
    .expect("execute Driver2.check");
    match &r3.outcome {
        ProgramOutcome::Returned(v) => {
            let fields: Vec<i32> = (0..4)
                .map(|i| {
                    let item = match v {
                        Value::Aggregate { fields, .. } => &fields[i],
                        _ => h3.field(v, i).expect("tuple field"),
                    };
                    match item {
                        Value::Int(n) => *n,
                        other => panic!("expected Int at tuple.{i}, got {other:?}"),
                    }
                })
                .collect();
            assert_eq!(fields, vec![1, 2, 2, 42]);
        }
        other => panic!("expected Returned tuple, got {other:?}"),
    }
    r3.outcome.release(&mut h3).expect("release r3");
    assert_eq!(h3.finish().expect("zero leaks h3").live_objects, 0);

    let loaded_iface = s2.module_interface("Counter").expect("Counter interface");
    assert_eq!(
        iface::fingerprint(loaded_iface),
        iface::fingerprint(expected_iface)
    );

    let _ = s2.unload_module("Driver2").expect("unload Driver2");
    let u_report = s2.unload_module("Counter").expect("unload Counter");
    assert_eq!(u_report.remaining_live_slots, 0);
    assert_eq!(s2.heap_inventory().live_slots_for("Counter"), 0);
}

#[test]
fn flick_source_closures_partial_application_and_shared_ref_counter_survive_save_load() {
    let source = "\
module Counter exposing (make_counter, add, add10, pair)

import Ref

make_counter : Unit ->{} (Unit ->{} Int, Unit ->{} Int)
let make_counter () =
  let counter = Ref.make 0
  let step () =
    let cur = Ref.get counter
    let _ = Ref.set counter (cur + 1)
    Ref.get counter
  let read () = Ref.get counter
  (step, read)

add : Int -> Int -> Int
let add x y = x + y

add10 : Int -> Int
let add10 = add 10

let shared = \"hello-shared\"
let pair = (shared, shared)
";
    let entries = [
        Name::new("Counter", "make_counter"),
        Name::new("Counter", "add"),
        Name::new("Counter", "add10"),
        Name::new("Counter", "pair"),
    ];
    let (interfaces, program) = lower_source_files(&[("/Counter.fl", source)], &entries);

    // VM 1: initialize Counter, save only `Counter` to `.fvm` bytes, shut down VM 1.
    let mut s1 = VmSession::with_host_mode(HostMode::Deterministic);
    for iface in &interfaces {
        s1.register_module_interface(iface.clone());
    }
    let init = s1.eval_declarations(&program, "Counter", &entries, None, None);
    assert_eq!(
        init,
        EvalOutcome::Declaration {
            stdout: String::new()
        }
    );

    // Save `Counter` alone (depends on `Ref`) AND the whole session (`Counter` + `Ref`).
    let counter_only_bytes = s1
        .save_modules(SaveSelection::Modules(&["Counter"]), None)
        .expect("save Counter only")
        .encode()
        .expect("encode Counter only");
    let full_bytes = s1
        .save_image(None)
        .expect("save full session")
        .encode()
        .expect("encode full session");
    assert_eq!(s1.shutdown(), Ok(()));

    // Loading `Counter`-only into an empty VM cleanly refuses missing dependency `Ref`.
    let counter_only = VmImage::decode(&counter_only_bytes).expect("decode Counter only");
    let mut empty_vm = VmSession::with_host_mode(HostMode::Deterministic);
    let missing_err = empty_vm
        .load_image(&counter_only)
        .expect_err("must require Ref dependency");
    assert!(
        missing_err.to_string().contains("Ref"),
        "expected MissingDependency naming Ref, got: {missing_err}"
    );
    assert_eq!(empty_vm.shutdown(), Ok(()));

    // VM 2: load full image (`Counter` + `Ref`), construct a live counter, step twice, read, and call `add10 32`!
    let decoded = VmImage::decode(&full_bytes).expect("decode full");
    let mut s2 = VmSession::with_host_mode(HostMode::Deterministic);
    s2.load_image(&decoded).expect("load into VM 2");
    verify_loaded_counter_session(&mut s2, source, &interfaces[0]);
    assert_eq!(s2.shutdown(), Ok(()));
}
