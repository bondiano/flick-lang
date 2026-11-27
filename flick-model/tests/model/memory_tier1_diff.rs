//! Differential checks comparing `flick_vm::tier1` against `flick_vm::tier0`.
//!
//! 1. Every pure `tests/run` case without handlers, tasks, scopes, arenas,
//!    generators, layers, or foreign calls prepares on tier 1, prints its
//!    checked-in expected `.stdout`, matches tier 0's outcome, and leaves
//!    zero leaked slots on tier 1.
//! 2. Generated well-typed programs from `flick_model::generate` give the
//!    same outcome and structurally identical live-heap result on both tiers,
//!    with zero leaked slots on tier 1.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use flick_core::ir::memory::{
    Block, CodeId, Exit, HostPrimitive, Instruction, Intrinsic, Name, Operation, Program,
};
use flick_model::generate;
use flick_model::run;
use flick_model::syntax::Path as ModelPath;
use flick_model::types::Type;
use flick_test_support::files as fixture_files;
use flick_vm::stdlib;
use flick_vm::tier0::{
    Error as MemoryError, Heap, HostMode, ProgramOutcome, Value,
    execute_main_program_outcome_with_host as tier0_exec_main,
    execute_program_outcome as tier0_exec_program,
};
use flick_vm::tier1;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Walks code reachable from `entry` to determine whether a lowered program
/// falls within tier 1's pure synchronous subset (no handlers, tasks, scopes,
/// arenas, generators, layers, refs, async, or foreign calls).
fn is_pure_tier1_subset(program: &Program, entry: &Name) -> bool {
    let initializers: BTreeMap<&Name, CodeId> = program
        .globals
        .iter()
        .map(|g| (&g.name, g.initializer))
        .collect();
    let Some(&entry_code) = initializers.get(entry) else {
        return false;
    };
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::from([entry_code]);
    while let Some(code) = queue.pop_front() {
        if !visited.insert(code) {
            continue;
        }
        let Some(function) = program.functions.get(&code) else {
            return false;
        };
        if !block_is_pure(&function.body, &initializers, &mut queue) {
            return false;
        }
    }
    true
}

fn block_is_pure(
    block: &Block,
    initializers: &BTreeMap<&Name, CodeId>,
    queue: &mut VecDeque<CodeId>,
) -> bool {
    for instr in &block.instructions {
        let Instruction::Let { value, .. } = instr else {
            continue;
        };
        match value {
            Operation::Handle { .. }
            | Operation::Task { .. }
            | Operation::Await(_)
            | Operation::Within { .. }
            | Operation::WithOwnerEvidence { .. }
            | Operation::GenerateStrict { .. }
            | Operation::GenerateLazy { .. }
            | Operation::Yield(_)
            | Operation::SeqOpen(_)
            | Operation::SeqNext(_)
            | Operation::ForeignCall { .. } => return false,
            Operation::Perform {
                effect,
                op,
                dicts,
                args,
            } if !(effect == "Host.Console"
                && dicts.is_empty()
                && args.len() == 1
                && matches!(op.as_str(), "stdout_line" | "stdout_write" | "stderr_line")) =>
            {
                return false;
            }
            Operation::Intrinsic { kind, .. } if !intrinsic_is_tier1_supported(*kind) => {
                return false;
            }
            Operation::Global(name) => {
                let Some(&code) = initializers.get(name) else {
                    return false;
                };
                queue.push_back(code);
            }
            Operation::Closure { code, .. }
            | Operation::Call { code, .. }
            | Operation::Loop { code, .. }
            | Operation::FunctionBoundary { code, .. } => queue.push_back(*code),
            _ => {}
        }
    }
    match &block.exit {
        Exit::Return(_) => true,
        Exit::Branch { then_, else_, .. } => {
            block_is_pure(then_, initializers, queue) && block_is_pure(else_, initializers, queue)
        }
    }
}

fn intrinsic_is_tier1_supported(kind: Intrinsic) -> bool {
    stdlib::is_metered_primitive(kind)
        || matches!(
            kind,
            Intrinsic::StringStartsWith
                | Intrinsic::StringIncludes
                | Intrinsic::PrintLine
                | Intrinsic::Show
                | Intrinsic::MatchFail
                | Intrinsic::FromResult
                | Intrinsic::Raise
                | Intrinsic::EffectDie
                | Intrinsic::FailureRethrow
                | Intrinsic::Try
                | Intrinsic::Host(
                    HostPrimitive::StderrLine
                        | HostPrimitive::StdoutWrite
                        | HostPrimitive::Env
                        | HostPrimitive::Vars
                        | HostPrimitive::Args
                        | HostPrimitive::CurrentTimeMillis
                        | HostPrimitive::CurrentTimeNanos
                        | HostPrimitive::MonotonicTimeNanos
                        | HostPrimitive::RandomNext
                )
        )
}

fn outcome_tag(outcome: &ProgramOutcome) -> &'static str {
    match outcome {
        ProgramOutcome::Returned(Value::Unit) => "returned_unit",
        ProgramOutcome::Returned(_) => "returned_value",
        ProgramOutcome::Raised(_) => "raised",
        ProgramOutcome::Failed(_) => "failed",
        ProgramOutcome::Fault(MemoryError::OutOfFuel) => "out_of_fuel",
        ProgramOutcome::Fault(_) => "fault",
    }
}

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
            // Compare strings, BigInts, or rendered show values / tags + fields
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
            // Records, closures, dictionaries, or other heap objects: compare via show_value when available.
            match (stdlib::show_value(h0, v0), stdlib::show_value(h1, v1)) {
                (Ok(s0), Ok(s1)) => Ok(s0 == s1),
                (Err(_), Err(_)) => Ok(true),
                (r0, r1) => Err(format!("show mismatch: {r0:?} vs {r1:?}")),
            }
        }
        _ => Ok(false),
    }
}

fn check_run_profile(
    counted: &run::CountedMain,
    rel: &str,
    profile: &str,
    expected_stdout: Option<&str>,
    expects_nonzero: bool,
    fuel: u64,
    failures: &mut Vec<String>,
) {
    let mut heap0 = Heap::new();
    let r0 = counted.execute(&mut heap0, fuel);
    let tag0 = outcome_tag(&r0.outcome);

    let mut heap1 = Heap::new();
    let r1 = match tier1::execute_main_program_outcome(
        &mut heap1,
        &counted.program,
        &counted.entry,
        fuel,
    ) {
        Ok(report) => report,
        Err(err) => {
            let _ = r0.outcome.release(&mut heap0);
            let _ = heap0.finish();
            failures.push(format!("{rel} ({profile}): tier1 prepare failed: {err}"));
            return;
        }
    };
    let tag1 = outcome_tag(&r1.outcome);

    if tag0 != "out_of_fuel" && tag1 != "out_of_fuel" {
        if tag0 != tag1 {
            failures.push(format!(
                "{rel} ({profile}): outcome mismatch tier0={tag0} tier1={tag1}"
            ));
        }
        if r0.stdout != r1.stdout {
            failures.push(format!(
                "{rel} ({profile}): stdout mismatch tier0={:?} tier1={:?}",
                r0.stdout, r1.stdout
            ));
        }
        if let Some(expected) = expected_stdout
            && !expects_nonzero
            && r1.stdout != expected
        {
            failures.push(format!(
                "{rel} ({profile}): checked-in stdout mismatch expected={expected:?} tier1={:?}",
                r1.stdout
            ));
        }
    }

    if let Err(err) = r0.outcome.release(&mut heap0) {
        failures.push(format!("{rel} ({profile}): tier0 release failed: {err}"));
    }
    if let Err(err) = heap0.finish() {
        failures.push(format!("{rel} ({profile}): tier0 leak: {err}"));
    }
    if let Err(err) = r1.outcome.release(&mut heap1) {
        failures.push(format!("{rel} ({profile}): tier1 release failed: {err}"));
    }
    match heap1.finish() {
        Ok(stats) if stats.live_objects == 0 && stats.allocations == stats.frees => {}
        Ok(stats) => {
            failures.push(format!("{rel} ({profile}): tier1 leak stats={stats:?}"));
        }
        Err(err) => failures.push(format!("{rel} ({profile}): tier1 leak: {err}")),
    }
}

#[test]
fn pure_tests_run_cases_match_tier0_and_checked_in_stdout() {
    let run_dir = repo().join("tests/run");
    let mut files = fixture_files(&run_dir, "fl");
    files.sort();
    let fuel: u64 = if cfg!(debug_assertions) {
        1_000_000
    } else {
        20_000_000
    };

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for path in &files {
        let rel = path
            .strip_prefix(repo())
            .unwrap_or(path)
            .display()
            .to_string();
        let Ok(modules) = run::compile(path) else {
            continue;
        };
        let Ok(main) = run::main_of(&modules) else {
            continue;
        };
        let expected_stdout = fs::read_to_string(path.with_extension("stdout")).ok();
        let expects_nonzero = fs::read_to_string(path.with_extension("exit"))
            .ok()
            .is_some_and(|s| s.trim() != "0");

        for release in [false, true] {
            let profile = if release { "release" } else { "debug" };
            let Ok(counted) = run::counted_main(&modules, &main, release) else {
                continue;
            };
            if !counted.eager_inits.is_empty()
                || !is_pure_tier1_subset(&counted.program, &counted.entry)
            {
                continue;
            }
            check_run_profile(
                &counted,
                &rel,
                profile,
                expected_stdout.as_deref(),
                expects_nonzero,
                fuel,
                &mut failures,
            );
            checked += 1;
        }
    }

    assert!(
        failures.is_empty(),
        "tier1 differential failures ({} of {checked} checked):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        checked >= 100,
        "expected at least 100 pure tests/run profiles to run on tier1, got {checked}"
    );
}

fn compare_generated_profile(
    counted: &run::CountedMain,
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
    let mut heap1 = Heap::new();
    let r1 = if is_unit_fn {
        tier1::execute_main_program_outcome_with_host(
            &mut heap1,
            &counted.program,
            &counted.entry,
            fuel,
            HostMode::Deterministic,
        )
    } else {
        tier1::execute_program_outcome(&mut heap1, &counted.program, &counted.entry, vec![], fuel)
    }
    .unwrap_or_else(|err| {
        panic!("seed {seed} (release={release}) failed tier1 prepare: {err}\n{source}")
    });

    assert_eq!(
        r0.stdout, r1.stdout,
        "seed {seed} (release={release}) stdout mismatch\n{source}"
    );
    match (&r0.outcome, &r1.outcome) {
        (ProgramOutcome::Returned(v0), ProgramOutcome::Returned(v1))
        | (ProgramOutcome::Raised(v0), ProgramOutcome::Raised(v1)) => {
            let eq = values_structurally_equal(&heap0, v0, &heap1, v1).unwrap_or_else(|e| {
                panic!("seed {seed} (release={release}) value compare error: {e}\n{source}")
            });
            assert!(
                eq,
                "seed {seed} (release={release}) returned value mismatch:\ntier0={v0:?}\ntier1={v1:?}\n{source}"
            );
        }
        (ProgramOutcome::Failed(_), ProgramOutcome::Failed(_))
        | (
            ProgramOutcome::Fault(MemoryError::OutOfFuel),
            ProgramOutcome::Fault(MemoryError::OutOfFuel),
        ) => {}
        (o0, o1) => {
            panic!(
                "seed {seed} (release={release}) outcome mismatch:\ntier0={o0:?}\ntier1={o1:?}\n{source}"
            );
        }
    }
    r0.outcome.release(&mut heap0).expect("release tier0");
    heap0.finish().expect("zero leaks tier0");
    r1.outcome.release(&mut heap1).expect("release tier1");
    let stats = heap1.finish().expect("zero leaks tier1");
    assert_eq!(stats.live_objects, 0);
    assert_eq!(stats.allocations, stats.frees);
}

#[test]
fn generated_programs_match_tier0_outcome_and_value_with_zero_leaks() {
    let mut compared = 0usize;
    let mut rng_state: u64 = 0x9740_cafe_1234_5678;
    let mut next_u32 = || {
        rng_state = rng_state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        u32::try_from(rng_state >> 32).expect("shifted u64 fits u32")
    };
    let fuel = 100_000;
    let entry_path = ModelPath::new("Fixture", "program");

    for seed in 0..400 {
        let len = (next_u32() as usize % 40) + 6;
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
        for release in [false, true] {
            let Ok(counted) = run::counted_main(&modules, &entry_path, release) else {
                continue;
            };
            if !is_pure_tier1_subset(&counted.program, &counted.entry) {
                continue;
            }
            compare_generated_profile(&counted, is_unit_fn, seed, release, &source, fuel);
            compared += 1;
        }
    }

    assert!(
        compared >= 40,
        "expected at least 40 generated program differential runs, got {compared}"
    );
}
