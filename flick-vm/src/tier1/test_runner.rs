//! Tier-1 bytecode test-tree executor for `List Test.Test` suites.
//!
//! Runs pure, effectful (each in its own lexical `Scope` owner), virtual-clock,
//! and seeded 100-case property tests on the tier-1 register machine without
//! leaking heap slots or native host handles.

use flick_core::ir::memory::{Name, Program};

use crate::sandbox::verify_bytecode;
use crate::stdlib::{DEFAULT_VIRTUAL_NOW_NS, FIBER_QUANTUM, primitives};
use crate::tier0::program::test_helpers::{self, skipped_case};
use crate::tier0::{
    CauseToken, Error, Heap, HostMode, LanguageExit, ProgramOutcome, TestCaseResult, Value,
};

use super::machine::{End, Machine, MainStage, OwnerEvidence, RegVal};
use super::machine_scopes::OwnerState;
use super::opcode::{PrepareError, Prepared};
use super::prepare::{prepare, prepare_rooted};

/// Prepares `program` for test suite `entry` (with optional `Host.live` `root`)
/// and executes all test cases under [`HostMode::Deterministic`].
///
/// Caller verifies `heap.finish()` for zero live heap slots.
///
/// # Errors
/// Returns [`PrepareError`] if the suite fails tier-1 preparation.
pub fn execute_tests_program(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    seed: i32,
    fuel_per_test: u64,
    skip_groups: &[&str],
) -> Result<Result<Vec<TestCaseResult>, Error>, PrepareError> {
    execute_tests_program_with_host(
        heap,
        program,
        entry,
        root,
        seed,
        fuel_per_test,
        skip_groups,
        HostMode::Deterministic,
    )
}

/// Prepares `program` for test suite `entry` (with optional `Host.live` `root`)
/// and executes all test cases under an explicit [`HostMode`].
///
/// Caller verifies `heap.finish()` for zero live heap slots. Native OS handles
/// acquired inside tests must all be released before the suite finishes.
///
/// # Errors
/// Returns [`PrepareError`] if the suite fails tier-1 preparation.
#[expect(
    clippy::too_many_arguments,
    reason = "matches tier-0 test runner signature"
)]
pub fn execute_tests_program_with_host(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    seed: i32,
    fuel_per_test: u64,
    skip_groups: &[&str],
    host_mode: HostMode,
) -> Result<Result<Vec<TestCaseResult>, Error>, PrepareError> {
    let image = match root {
        Some(root_name) => prepare_rooted(program, entry, root_name)?,
        None => prepare(program, entry)?,
    };
    Ok(execute_prepared_tests(
        heap,
        &image,
        seed,
        fuel_per_test,
        skip_groups,
        host_mode,
    ))
}

/// Executes a pre-prepared tier-1 test suite image under `host_mode`.
///
/// # Errors
/// Returns [`Error`] on bytecode verification failure, suite init failure,
/// unsupported test nodes, fuel exhaustion, or leaked native host handles.
pub fn execute_prepared_tests(
    heap: &mut Heap,
    image: &Prepared,
    seed: i32,
    fuel_per_test: u64,
    skip_groups: &[&str],
    host_mode: HostMode,
) -> Result<Vec<TestCaseResult>, Error> {
    if let Err(err) = verify_bytecode(image) {
        return Err(Error::VerificationFailed(err.to_string()));
    }
    let suite_module = image.global_names[image.entry].module.clone();
    let mut machine = Machine::new(
        heap,
        image,
        Vec::new(),
        fuel_per_test,
        host_mode,
        Vec::new(),
        false,
    );
    if image.root.is_some() {
        machine.main_stage = MainStage::RootValue { main: image.entry };
    }
    let tests_val = match machine.start_entry().and_then(|()| machine.run()) {
        Ok(LanguageExit::Success(val)) => val,
        Ok(LanguageExit::Failure(cause)) => {
            let msg = machine
                .describe_test_failure(&cause)
                .unwrap_or_else(|e| e.to_string());
            let drop_err = machine.heap.drop_cause(cause);
            let outcome = machine.finish(drop_err.map(|()| LanguageExit::Success(Value::Unit)));
            let _ = outcome.release(heap);
            return Err(Error::Assert(format!("suite init failed: {msg}")));
        }
        Err(err) => {
            let outcome = machine.finish(Err(err));
            match outcome {
                ProgramOutcome::Fault(fault) => return Err(fault),
                other => {
                    let _ = other.release(heap);
                    return Err(Error::UnsupportedOperation(
                        "test suite initialization fault",
                    ));
                }
            }
        }
    };

    let mut results = Vec::new();
    let walk_res = machine.run_test_list(
        &suite_module,
        &tests_val,
        seed,
        fuel_per_test,
        skip_groups,
        false,
        &mut results,
    );
    let drop_res = machine.heap.drop_value(tests_val);
    let leaked_host_handles = if walk_res.is_ok() {
        machine.host_services.live_handle_count()
    } else {
        0
    };
    let final_res = walk_res
        .and(drop_res)
        .map(|()| LanguageExit::Success(Value::Unit));
    let outcome = machine.finish(final_res);
    match outcome {
        ProgramOutcome::Fault(err) => Err(err),
        other => {
            other.release(heap)?;
            if leaked_host_handles > 0 {
                return Err(Error::LeakedForeignHandles(leaked_host_handles));
            }
            Ok(results)
        }
    }
}

impl Machine<'_, '_> {
    #[expect(clippy::too_many_arguments, reason = "recursive test tree walk")]
    fn run_test_list(
        &mut self,
        prefix: &str,
        list_val: &Value,
        seed: i32,
        fuel_per_test: u64,
        skip_groups: &[&str],
        skipped: bool,
        out: &mut Vec<TestCaseResult>,
    ) -> Result<(), Error> {
        let items = self.list_reps(list_val)?;
        for item in &items {
            self.run_test_node(prefix, item, seed, fuel_per_test, skip_groups, skipped, out)?;
        }
        Ok(())
    }

    #[expect(clippy::too_many_arguments, reason = "recursive test tree walk")]
    fn run_test_node(
        &mut self,
        prefix: &str,
        node: &Value,
        seed: i32,
        fuel_per_test: u64,
        skip_groups: &[&str],
        skipped: bool,
        out: &mut Vec<TestCaseResult>,
    ) -> Result<(), Error> {
        let tag = self.heap.tag(node)?.to_owned();
        match tag.as_str() {
            "Test.Test.$Group" => {
                let name = self.string_field(node, 0)?;
                let children = self.heap.field(node, 1)?.representation();
                let full = format!("{prefix} > {name}");
                let skip_here = skipped || skip_groups.contains(&name.as_str());
                self.run_test_list(
                    &full,
                    &children,
                    seed,
                    fuel_per_test,
                    skip_groups,
                    skip_here,
                    out,
                )
            }
            "Test.Test.$Skip" => {
                let inner = self.heap.field(node, 0)?.representation();
                self.run_test_node(prefix, &inner, seed, fuel_per_test, skip_groups, true, out)
            }
            "Test.Test.$Only" | "Test.Test.$Serial" => {
                let inner = self.heap.field(node, 0)?.representation();
                self.run_test_node(
                    prefix,
                    &inner,
                    seed,
                    fuel_per_test,
                    skip_groups,
                    skipped,
                    out,
                )
            }
            "Test.Test.$Timeout" => {
                let inner = self.heap.field(node, 1)?.representation();
                self.run_test_node(
                    prefix,
                    &inner,
                    seed,
                    fuel_per_test,
                    skip_groups,
                    skipped,
                    out,
                )
            }
            "Test.Test.$Pure" => {
                let name = format!("{prefix} > {}", self.string_field(node, 0)?);
                if skipped {
                    out.push(skipped_case(name));
                    return Ok(());
                }
                let body = self.heap.project(node, 1)?;
                out.push(self.exec_pure_test(name, body, fuel_per_test)?);
                Ok(())
            }
            "Test.Test.$Effect" | "Test.Test.$Clock" => {
                let is_clock = tag == "Test.Test.$Clock";
                let name = format!("{prefix} > {}", self.string_field(node, 0)?);
                if skipped {
                    out.push(skipped_case(name));
                    return Ok(());
                }
                let task = self.heap.project(node, 1)?;
                out.push(self.exec_effect_test(name, task, fuel_per_test, is_clock)?);
                Ok(())
            }
            "Test.Test.$Property" => {
                let name = format!("{prefix} > {}", self.string_field(node, 0)?);
                if skipped {
                    out.push(skipped_case(name));
                    return Ok(());
                }
                let gen_fn = self.heap.project(node, 1)?;
                let check_fn = self.heap.project(node, 2)?;
                out.push(self.exec_property_test(name, gen_fn, check_fn, seed, fuel_per_test)?);
                Ok(())
            }
            "Test.Test.$Suite"
            | "Test.Test.$BeforeAll"
            | "Test.Test.$BeforeEach"
            | "Test.Test.$AfterEach"
            | "Test.Test.$AfterAll"
            | "Test.Test.$Share" => {
                if skipped {
                    return Ok(());
                }
                Err(Error::UnsupportedOperation(
                    "unsupported test node kind in VM test runner",
                ))
            }
            _ => Err(Error::UnsupportedOperation("unknown test node tag")),
        }
    }

    fn exec_pure_test(
        &mut self,
        name: String,
        body: Value,
        fuel: u64,
    ) -> Result<TestCaseResult, Error> {
        self.reset_test_state(fuel);
        self.apply_reg(
            RegVal::Value(body),
            RegVal::Value(Value::Unit),
            End::Top,
            false,
        )?;
        let exit = self.run()?;
        self.collect_assertion_outcome(name, exit)
    }

    fn exec_effect_test(
        &mut self,
        name: String,
        task: Value,
        fuel: u64,
        clock_mode: bool,
    ) -> Result<TestCaseResult, Error> {
        self.reset_test_state(fuel);
        self.test_clock_mode = clock_mode;
        self.virtual_now_ns = if clock_mode {
            0
        } else {
            DEFAULT_VIRTUAL_NOW_NS
        };
        let previous = self.current_owner;
        let owner = self.owners.len();
        self.owners.push(OwnerState::default());
        self.current_owner = owner;
        let saved_evidence = self.current_evidence;
        self.current_evidence = Some(OwnerEvidence::from_owner(&self.owner_handle(owner)));
        self.await_task_reg(
            RegVal::Value(task),
            End::CloseScope {
                owner,
                previous,
                dst: None,
            },
        )?;
        let run_res = self.run();
        self.current_evidence = saved_evidence;
        self.test_clock_mode = false;
        match run_res? {
            LanguageExit::Success(val) => {
                self.heap.drop_value(val)?;
                Ok(TestCaseResult {
                    name,
                    passed: true,
                    skipped: false,
                    message: None,
                })
            }
            LanguageExit::Failure(cause) => {
                let msg = self.describe_test_failure(&cause)?;
                self.heap.drop_cause(cause)?;
                Ok(TestCaseResult {
                    name,
                    passed: false,
                    skipped: false,
                    message: Some(msg),
                })
            }
        }
    }

    #[expect(
        clippy::cast_possible_wrap,
        reason = "exact 32-bit seed stride matching JS/Go property runner"
    )]
    fn exec_property_test(
        &mut self,
        name: String,
        gen_fn: Value,
        check_fn: Value,
        seed: i32,
        fuel: u64,
    ) -> Result<TestCaseResult, Error> {
        let mut outcome: Result<Option<String>, Error> = Ok(None);
        for case_idx in 0_i32..100 {
            let case_seed = seed.wrapping_add(case_idx.wrapping_mul(0x9e37_79b9_u32 as i32));
            match self.run_property_case(&name, &gen_fn, &check_fn, case_idx, case_seed, fuel) {
                Ok(None) => {}
                Ok(Some(msg)) => {
                    outcome = Ok(Some(msg));
                    break;
                }
                Err(err) => {
                    outcome = Err(err);
                    break;
                }
            }
        }
        let drop_res = self.heap.drop_values(vec![gen_fn, check_fn]);
        let failure_msg = outcome?;
        drop_res?;
        Ok(TestCaseResult {
            name,
            passed: failure_msg.is_none(),
            skipped: false,
            message: failure_msg,
        })
    }

    fn run_property_case(
        &mut self,
        name: &str,
        gen_fn: &Value,
        check_fn: &Value,
        case_idx: i32,
        case_seed: i32,
        fuel: u64,
    ) -> Result<Option<String>, Error> {
        let g = self.heap.dup(gen_fn)?;
        self.reset_test_state(fuel);
        self.apply_reg(
            RegVal::Value(g),
            RegVal::Value(Value::Int(case_seed)),
            End::Top,
            false,
        )?;
        let pair = match self.run()? {
            LanguageExit::Success(val) => val,
            LanguageExit::Failure(cause) => {
                let msg = self.describe_test_failure(&cause)?;
                self.heap.drop_cause(cause)?;
                return Ok(Some(format!("generator failed at case {case_idx}: {msg}")));
            }
        };
        let drawn = match self.heap.project(&pair, 1) {
            Ok(v) => v,
            Err(err) => {
                self.heap.drop_value(pair)?;
                return Err(err);
            }
        };
        self.heap.drop_value(pair)?;

        let drawn_copy = match self.heap.dup(&drawn) {
            Ok(v) => v,
            Err(err) => {
                self.heap.drop_value(drawn)?;
                return Err(err);
            }
        };
        let c = match self.heap.dup(check_fn) {
            Ok(v) => v,
            Err(err) => {
                self.heap.drop_values(vec![drawn, drawn_copy])?;
                return Err(err);
            }
        };
        self.reset_test_state(fuel);
        if let Err(err) = self.apply_reg(RegVal::Value(c), RegVal::Value(drawn), End::Top, false) {
            self.heap.drop_value(drawn_copy)?;
            return Err(err);
        }
        let check_exit = match self.run() {
            Ok(exit) => exit,
            Err(err) => {
                self.heap.drop_value(drawn_copy)?;
                return Err(err);
            }
        };
        let case_outcome = match self.collect_assertion_outcome(name.to_owned(), check_exit) {
            Ok(outcome) => outcome,
            Err(err) => {
                self.heap.drop_value(drawn_copy)?;
                return Err(err);
            }
        };
        if case_outcome.passed {
            self.heap.drop_value(drawn_copy)?;
            Ok(None)
        } else {
            let shown = primitives::preview_property_value(self.heap, &drawn_copy);
            self.heap.drop_value(drawn_copy)?;
            let shown = shown?;
            Ok(Some(format!(
                "{} (case {case_idx}: {shown})",
                case_outcome.message.unwrap_or_default()
            )))
        }
    }

    fn reset_test_state(&mut self, fuel: u64) {
        self.fuel = fuel;
        self.main_stage = MainStage::Done;
        self.failure_elected = false;
        self.active_fiber = 1;
        self.quantum = FIBER_QUANTUM;
        self.test_clock_mode = false;
        self.virtual_now_ns = DEFAULT_VIRTUAL_NOW_NS;
        self.test_clock_waiter = None;
        self.timers.clear();
        self.mask_depth = 0;
        self.interrupt_requested = None;
        self.pending_resume_close = None;
    }

    fn collect_assertion_outcome(
        &mut self,
        name: String,
        exit: LanguageExit,
    ) -> Result<TestCaseResult, Error> {
        test_helpers::collect_assertion_outcome(self.heap, name, exit)
    }

    fn describe_test_failure(&self, cause: &CauseToken) -> Result<String, Error> {
        test_helpers::describe_test_failure(self.heap, cause)
    }

    fn string_field(&self, val: &Value, index: usize) -> Result<String, Error> {
        test_helpers::string_field(self.heap, val, index)
    }

    fn list_reps(&self, val: &Value) -> Result<Vec<Value>, Error> {
        test_helpers::list_reps(self.heap, val)
    }
}
