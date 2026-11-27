// Counted test-tree executor for `List Test.Test` suites.
// Runs pure, effectful (each in its own lexical `Scope` owner), and seeded
// 100-case property tests through the real counted `Machine` without leaking.

/// Outcome of one leaf test executed by [`execute_tests_program`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCaseResult {
    pub name: String,
    pub passed: bool,
    pub skipped: bool,
    pub message: Option<String>,
}

/// Evaluates `entry` (typically `Language.<Suite>.tests`) with optional root
/// layer `root` (`Host.live`), walks the resulting `Test.$*` tree, executes
/// every test case under `fuel_per_test`, and releases all machine roots.
/// Caller verifies `heap.finish()` for zero leaks.
pub fn execute_tests_program(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    seed: i32,
    fuel_per_test: u64,
    skip_groups: &[&str],
) -> Result<Vec<TestCaseResult>, Error> {
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

/// Evaluates and runs a `List Test.Test` suite with an explicit [`HostMode`].
#[expect(clippy::too_many_arguments, reason = "explicit host-mode test runner")]
pub fn execute_tests_program_with_host(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    seed: i32,
    fuel_per_test: u64,
    skip_groups: &[&str],
    host_mode: HostMode,
) -> Result<Vec<TestCaseResult>, Error> {
    #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
    {
        execute_tests_program_inner(
            heap,
            program,
            entry,
            root,
            seed,
            fuel_per_test,
            skip_groups,
            host_mode,
            None,
        )
    }
    #[cfg(not(all(feature = "native-host", not(target_arch = "wasm32"))))]
    {
        execute_tests_program_inner(
            heap,
            program,
            entry,
            root,
            seed,
            fuel_per_test,
            skip_groups,
            host_mode,
        )
    }
}

/// Evaluates and runs a `List Test.Test` suite with an explicit [`HostMode`]
/// and [`HostBridge`], releasing all Rust heap slots and host handles on return.
#[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
#[expect(clippy::too_many_arguments, reason = "explicit host-bridged test runner")]
pub fn execute_tests_program_with_bridge(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    seed: i32,
    fuel_per_test: u64,
    skip_groups: &[&str],
    host_mode: HostMode,
    bridge: &mut HostBridge,
) -> Result<Vec<TestCaseResult>, Error> {
    execute_tests_program_inner(
        heap,
        program,
        entry,
        root,
        seed,
        fuel_per_test,
        skip_groups,
        host_mode,
        Some(bridge),
    )
}

#[expect(clippy::too_many_arguments, reason = "shared test-suite executor")]
fn execute_tests_program_inner(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    seed: i32,
    fuel_per_test: u64,
    skip_groups: &[&str],
    host_mode: HostMode,
    #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))] bridge: Option<&mut HostBridge>,
) -> Result<Vec<TestCaseResult>, Error> {
    if let Err(err) = memory::validate_program(program) {
        return Err(Error::InvalidProgram(err));
    }
    if !program.globals.iter().any(|g| &g.name == entry) {
        return Err(Error::UnknownGlobal(entry.clone()));
    }
    let start_global = root.unwrap_or(entry);
    let main_stage = match root {
        Some(_) => MainStage::Root {
            main: entry.clone(),
            eager_inits: VecDeque::new(),
        },
        None => MainStage::Done,
    };
    let execution = NEXT_EXECUTION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("fewer than usize::MAX model executions");
    let mut machine = Machine::new(heap, program, execution, vec![], fuel_per_test, main_stage);
    machine.host_mode = host_mode;
    #[cfg(all(feature = "native-host", not(target_arch = "wasm32")))]
    {
        machine.bridge = bridge;
    }
    let tests_val = match machine.global(start_global, None).and_then(|()| machine.run()) {
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
                    return Err(Error::UnsupportedOperation("test suite initialization fault"));
                }
            }
        }
    };

    let mut results = Vec::new();
    let walk_res = machine.run_test_list(
        &entry.module,
        &tests_val,
        seed,
        fuel_per_test,
        skip_groups,
        false,
        &mut results,
    );
    let drop_res = machine.heap.drop_value(tests_val);
    let final_res = walk_res
        .and(drop_res)
        .map(|()| LanguageExit::Success(Value::Unit));
    let outcome = machine.finish(final_res);
    match outcome {
        ProgramOutcome::Fault(err) => Err(err),
        other => {
            other.release(heap)?;
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
                self.run_test_node(prefix, &inner, seed, fuel_per_test, skip_groups, skipped, out)
            }
            "Test.Test.$Timeout" => {
                let inner = self.heap.field(node, 1)?.representation();
                self.run_test_node(prefix, &inner, seed, fuel_per_test, skip_groups, skipped, out)
            }
            "Test.Test.$Pure" => {
                let name = format!("{prefix} > {}", self.string_field(node, 0)?);
                if skipped {
                    out.push(TestCaseResult {
                        name,
                        passed: true,
                        skipped: true,
                        message: None,
                    });
                    return Ok(());
                }
                let body = self.heap.project(node, 1)?;
                let res = self.exec_pure_test(name, body, fuel_per_test)?;
                out.push(res);
                Ok(())
            }
            "Test.Test.$Effect" | "Test.Test.$Clock" => {
                let is_clock = tag == "Test.Test.$Clock";
                let name = format!("{prefix} > {}", self.string_field(node, 0)?);
                if skipped {
                    out.push(TestCaseResult {
                        name,
                        passed: true,
                        skipped: true,
                        message: None,
                    });
                    return Ok(());
                }
                let task = self.heap.project(node, 1)?;
                let res = self.exec_effect_test(name, task, fuel_per_test, is_clock)?;
                out.push(res);
                Ok(())
            }
            "Test.Test.$Property" => {
                let name = format!("{prefix} > {}", self.string_field(node, 0)?);
                if skipped {
                    out.push(TestCaseResult {
                        name,
                        passed: true,
                        skipped: true,
                        message: None,
                    });
                    return Ok(());
                }
                let gen_fn = self.heap.project(node, 1)?;
                let check_fn = self.heap.project(node, 2)?;
                let res = self.exec_property_test(name, gen_fn, check_fn, seed, fuel_per_test)?;
                out.push(res);
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
                    "unsupported test node kind in Stage 3 runner",
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
        self.scratch.extend([body, Value::Unit]);
        self.apply_fresh(Destination::Top)?;
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
        self.scratch.extend([task, Value::Unit]);
        self.await_task_fresh(Destination::CloseScope {
            owner,
            previous,
            binding: None,
        })?;
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
        let mut failure_msg = None;
        for case_idx in 0_i32..100 {
            let case_seed = seed.wrapping_add(case_idx.wrapping_mul(0x9e37_79b9_u32 as i32));
            let g = self.heap.dup(&gen_fn)?;
            self.reset_test_state(fuel);
            self.scratch.extend([g, Value::Int(case_seed)]);
            self.apply_fresh(Destination::Top)?;
            let gen_exit = match self.run() {
                Ok(exit) => exit,
                Err(err) => {
                    self.heap.drop_values(vec![gen_fn, check_fn])?;
                    return Err(err);
                }
            };
            let pair = match gen_exit {
                LanguageExit::Success(val) => val,
                LanguageExit::Failure(cause) => {
                    let msg = self.describe_test_failure(&cause)?;
                    self.heap.drop_cause(cause)?;
                    failure_msg = Some(format!("generator failed at case {case_idx}: {msg}"));
                    break;
                }
            };
            let drawn = match self.heap.project(&pair, 1) {
                Ok(v) => v,
                Err(err) => {
                    self.heap.drop_values(vec![pair, gen_fn, check_fn])?;
                    return Err(err);
                }
            };
            self.heap.drop_value(pair)?;

            let drawn_copy = match self.heap.dup(&drawn) {
                Ok(v) => v,
                Err(err) => {
                    self.heap.drop_values(vec![drawn, gen_fn, check_fn])?;
                    return Err(err);
                }
            };
            let c = match self.heap.dup(&check_fn) {
                Ok(v) => v,
                Err(err) => {
                    self.heap
                        .drop_values(vec![drawn, drawn_copy, gen_fn, check_fn])?;
                    return Err(err);
                }
            };
            self.reset_test_state(fuel);
            self.scratch.extend([c, drawn]);
            self.apply_fresh(Destination::Top)?;
            let check_exit = match self.run() {
                Ok(exit) => exit,
                Err(err) => {
                    self.heap.drop_values(vec![drawn_copy, gen_fn, check_fn])?;
                    return Err(err);
                }
            };
            let case_outcome = match self.collect_assertion_outcome(name.clone(), check_exit) {
                Ok(outcome) => outcome,
                Err(err) => {
                    self.heap.drop_values(vec![drawn_copy, gen_fn, check_fn])?;
                    return Err(err);
                }
            };
            if !case_outcome.passed {
                let shown = primitives::preview_property_value(self.heap, &drawn_copy);
                self.heap.drop_value(drawn_copy)?;
                let shown = shown?;
                failure_msg = Some(format!(
                    "{} (case {case_idx}: {shown})",
                    case_outcome.message.unwrap_or_default()
                ));
                break;
            }
            self.heap.drop_value(drawn_copy)?;
        }
        self.heap.drop_values(vec![gen_fn, check_fn])?;
        Ok(TestCaseResult {
            name,
            passed: failure_msg.is_none(),
            skipped: false,
            message: failure_msg,
        })
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
