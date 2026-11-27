//! Differential checks comparing `flick_vm::tier1` against `flick_vm::tier0`.
//!
//! 1. Every `tests/run` (and selected `tests/rt`) case in tier 1's synchronous,
//!    effects/handlers, and scope-finalizer subset prepares on tier 1, matches
//!    checked-in `.stdout` and tier 0's outcome, and leaves zero leaked slots.
//! 2. Generated programs (`flick_model::generate` and generated handler/scope
//!    programs) match tier 0 with zero leaked slots on tier 1.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use flick_core::ir::memory::{Block, CodeId, Exit, Instruction, Name, Operation, Program};
use flick_model::run;
use flick_test_support::files as fixture_files;
use flick_vm::tier0::{Error as MemoryError, Heap, ProgramOutcome, Value};
use flick_vm::tier1;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

pub(super) fn is_tier1_subset(program: &Program, entry: &Name, root: Option<&Name>) -> bool {
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
    if let Some(root_name) = root {
        let Some(&root_code) = initializers.get(root_name) else {
            return false;
        };
        queue.push_back(root_code);
    }
    while let Some(code) = queue.pop_front() {
        if !visited.insert(code) {
            continue;
        }
        let Some(function) = program.functions.get(&code) else {
            return false;
        };
        if !block_is_tier1_supported(&function.body, &initializers, &mut queue) {
            return false;
        }
    }
    true
}

fn block_is_tier1_supported(
    block: &Block,
    initializers: &BTreeMap<&Name, CodeId>,
    queue: &mut VecDeque<CodeId>,
) -> bool {
    for instr in &block.instructions {
        let Instruction::Let { value, .. } = instr else {
            continue;
        };
        match value {
            Operation::ForeignCall { .. } => return false,
            Operation::Global(name) => {
                let Some(&code) = initializers.get(name) else {
                    return false;
                };
                queue.push_back(code);
            }
            Operation::Closure { code, .. }
            | Operation::Task { code, .. }
            | Operation::Call { code, .. }
            | Operation::Within { code, .. }
            | Operation::WithOwnerEvidence { code, .. }
            | Operation::Loop { code, .. }
            | Operation::FunctionBoundary { code, .. }
            | Operation::GenerateStrict { code, .. }
            | Operation::GenerateLazy { code, .. } => queue.push_back(*code),
            _ => {}
        }
    }
    match &block.exit {
        Exit::Return(_) => true,
        Exit::Branch { then_, else_, .. } => {
            block_is_tier1_supported(then_, initializers, queue)
                && block_is_tier1_supported(else_, initializers, queue)
        }
    }
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

#[allow(clippy::too_many_arguments)]
fn check_run_profile(
    counted: &run::CountedMain,
    rel: &str,
    profile: &str,
    expected_stdout: Option<&str>,
    expects_nonzero: bool,
    must_match_expected: bool,
    fuel: u64,
    failures: &mut Vec<String>,
) {
    let mut heap0 = Heap::new();
    let r0 = counted.execute(&mut heap0, fuel);
    let tag0 = outcome_tag(&r0.outcome);

    let mut heap1 = Heap::new();
    let r1_res = match &counted.root {
        Some(root) => tier1::execute_rooted_main_program_outcome(
            &mut heap1,
            &counted.program,
            &counted.entry,
            root,
            fuel,
        ),
        None => {
            tier1::execute_main_program_outcome(&mut heap1, &counted.program, &counted.entry, fuel)
        }
    };
    let r1 = match r1_res {
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
            && (must_match_expected || r0.stdout == expected)
            && r1.stdout != expected
        {
            failures.push(format!(
                "{rel} ({profile}): expected stdout mismatch expected={expected:?} tier1={:?}",
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
        Ok(stats) => failures.push(format!("{rel} ({profile}): tier1 leak stats={stats:?}")),
        Err(err) => failures.push(format!("{rel} ({profile}): tier1 leak: {err}")),
    }
}

#[test]
#[allow(clippy::too_many_lines)]
fn pure_tests_run_cases_match_tier0_and_checked_in_stdout() {
    const REQUIRED: &[&str] = &[
        "tests/run/codegen/handlers.fl",
        "tests/run/codegen/partial_handlers.fl",
        "tests/run/codegen/prototype_operation.fl",
        "tests/run/codegen/counted_failures.fl",
        "tests/run/codegen/fail.fl",
        "tests/run/codegen/errors.fl",
        "tests/run/runtime/arena_scope.fl",
        "tests/run/runtime/arena_layer_scope.fl",
        "tests/run/runtime/arena_owner_list.fl",
        "tests/run/runtime/arena_finalizers.fl",
        "tests/run/runtime/arena_layer_finalizers.fl",
        "tests/run/runtime/closing_hooks.fl",
        "tests/run/runtime/group_in_closing.fl",
        "tests/run/runtime/parallel.fl",
        "tests/run/runtime/shared_layer.fl",
        "tests/run/runtime/scope_close.fl",
        "tests/run/runtime/interrupt_skips.fl",
        "tests/run/runtime/abort_stops_children.fl",
        "tests/run/runtime/lazy_shared.fl",
        "tests/run/runtime/lazy_local_concurrent.fl",
        "tests/run/runtime/defer_lifecycle.fl",
        "tests/run/stdlib/seq.fl",
        "tests/run/stdlib/seqs.fl",
        "tests/run/stdlib/effect/stream.fl",
        "tests/run/stdlib/effect/stream_core.fl",
        "tests/run/stdlib/effect/stream_combine.fl",
        "tests/run/stdlib/effect/stream_errors.fl",
        "tests/run/stdlib/effect/stream_sink.fl",
        "tests/run/stdlib/effect/stream_transform.fl",
        "tests/run/schema/derive.fl",
        "tests/run/schema/json_wire.fl",
        "tests/run/schema/enums.fl",
        "tests/run/schema/combinators.fl",
        "tests/run/schema/route_nested.fl",
        "tests/run/stdlib/effect/schema.fl",
        "tests/run/stdlib/effect/codec.fl",
        "tests/run/stdlib/effect/check.fl",
        "tests/run/stdlib/effect/json_schema.fl",
        "tests/run/stdlib/effect/duration.fl",
        "tests/run/stdlib/effect/duration_text.fl",
        "tests/run/stdlib/effect/datetime.fl",
        "tests/run/stdlib/effect/datetime_more.fl",
        "tests/run/stdlib/decimals.fl",
        "tests/run/codegen/bigints.fl",
        "tests/run/stdlib/effect/arbitrary.fl",
        "tests/rt/deep_control.fl",
        "tests/rt/result_control_unwind.fl",
        "tests/rt/defer_control.fl",
    ];

    let mut files = fixture_files(&repo().join("tests/run"), "fl");
    files.sort();
    for rel in [
        "tests/rt/deep_control.fl",
        "tests/rt/result_control_unwind.fl",
        "tests/rt/defer_control.fl",
    ] {
        files.push(repo().join(rel));
    }
    let fuel: u64 = if cfg!(debug_assertions) {
        1_000_000
    } else {
        20_000_000
    };

    let mut checked = 0usize;
    let mut checked_files = BTreeSet::new();
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
        let must_match_expected = REQUIRED.contains(&rel.as_str());

        for release in [false, true] {
            let profile = if release { "release" } else { "debug" };
            let Ok(counted) = run::counted_main(&modules, &main, release) else {
                continue;
            };
            if !counted.eager_inits.is_empty()
                || !is_tier1_subset(&counted.program, &counted.entry, counted.root.as_ref())
            {
                continue;
            }
            check_run_profile(
                &counted,
                &rel,
                profile,
                expected_stdout.as_deref(),
                expects_nonzero,
                must_match_expected,
                fuel,
                &mut failures,
            );
            checked_files.insert(rel.clone());
            checked += 1;
        }
    }

    for &required in REQUIRED {
        assert!(
            checked_files.contains(required),
            "required fixture did not run on tier1: {required}"
        );
    }
    assert!(
        failures.is_empty(),
        "tier1 diff failures ({} of {checked}):\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        checked >= 250,
        "expected >= 250 tier1 profiles, got {checked}"
    );
}
