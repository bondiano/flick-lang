// Hot-swap steppable tier-0 execution and sandboxed unary migration runner
// built directly atop `SuspendedSessionEval` and lifetime-free `BlockLoc`.

/// Result of advancing a [`SuspendedTier0`] execution by up to `max_steps`.
#[derive(Debug)]
pub(crate) enum Tier0StepResult {
    Running { steps_taken: usize },
    Finished(ExecutionReport),
}

fn collect_frames_codes(frames: &[Frame], out: &mut BTreeSet<CodeId>) {
    for frame in frames {
        out.insert(frame.block.code);
    }
}

fn collect_discard_codes(discard: &Discard, out: &mut BTreeSet<CodeId>) {
    collect_frames_codes(&discard.frames, out);
    if let TransferNext::Chain(inner) = &discard.next {
        collect_discard_codes(inner, out);
    }
}

fn collect_closing_scope_codes(scope: &ClosingScopeState, out: &mut BTreeSet<CodeId>) {
    match &scope.outcome {
        ScopeOutcome::Discard(discard) => collect_discard_codes(discard, out),
        ScopeOutcome::ChildClose { parent, .. } => collect_closing_scope_codes(parent, out),
        ScopeOutcome::Normal { .. } | ScopeOutcome::Failure(_) => {}
    }
}

impl SuspendedSessionEval {
    /// Returns every `CodeId` still pinned by live frames, segments,
    /// continuations, generators, discards, closing scopes, or parked fibers.
    pub(crate) fn pinned_codes(&self) -> BTreeSet<CodeId> {
        let mut out = BTreeSet::new();
        collect_frames_codes(&self.frames, &mut out);
        for seg in self.segments.values() {
            collect_frames_codes(&seg.frames, &mut out);
        }
        for cont in self.continuations.values().flatten() {
            collect_frames_codes(&cont.frames, &mut out);
        }
        for gen_slot in self.generators.values() {
            match &gen_slot.run {
                GeneratorRun::Fresh { code, .. } => {
                    out.insert(*code);
                }
                GeneratorRun::Suspended { frames, .. } => collect_frames_codes(frames, &mut out),
                GeneratorRun::Running | GeneratorRun::Done => {}
            }
        }
        if let Some(discard) = &self.discard {
            collect_discard_codes(discard, &mut out);
        }
        for fiber in self.parked_fibers.values() {
            collect_frames_codes(&fiber.frames, &mut out);
            if let Some(discard) = &fiber.discard {
                collect_discard_codes(discard, &mut out);
            }
        }
        for scope in self.closing_scopes.values() {
            collect_closing_scope_codes(scope, &mut out);
        }
        out
    }
}

/// Owned, `'static` paused tier-0 VM execution that yields persistent globals
/// back to [`crate::session::VmSession`] at every pause so live hot swaps and
/// state migrations observe and update global cells atomically.
pub(crate) struct SuspendedTier0 {
    state: Option<Box<SuspendedSessionEval>>,
    initializing_globals: Vec<Name>,
    sandbox_config: SandboxConfig,
}

impl SuspendedTier0 {
    /// Returns every `CodeId` still referenced by suspended frames or fibers.
    pub(crate) fn pinned_codes(&self) -> BTreeSet<CodeId> {
        self.state
            .as_deref()
            .map(SuspendedSessionEval::pinned_codes)
            .unwrap_or_default()
    }
}

fn take_initializing_globals(machine: &Machine<'_, '_>) -> Vec<Name> {
    machine
        .globals
        .iter()
        .filter(|(_, opt)| opt.is_none())
        .map(|(name, _)| name.clone())
        .collect()
}

/// Evaluates `entry` and applies the resulting function to `argument` against
/// `persistent` under `sandbox` (used by atomic state-cell migrations).
#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_session_apply_unary(
    heap: &mut Heap,
    program: &Program,
    execution: usize,
    persistent: &mut SessionPersistentState,
    entry: &Name,
    argument: Value,
    host_mode: HostMode,
    sandbox: &SandboxConfig,
) -> ExecutionReport {
    if !program.globals.iter().any(|g| &g.name == entry) {
        let _ = heap.drop_value(argument);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let fuel = sandbox.limits.fuel.unwrap_or(u64::MAX);
    let mut machine = Machine::new(
        heap,
        program,
        execution,
        vec![argument],
        fuel,
        MainStage::Done,
    );
    machine.host_mode = host_mode;
    machine.configure_sandbox(sandbox);
    machine.load_persistent(persistent);
    let result = machine.global(entry, None).and_then(|()| machine.run());
    let outcome = machine.finish_session(result, persistent);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

/// Starts a new steppable tier-0 execution rooted at `entry`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn start_tier0_step(
    heap: &mut Heap,
    program: &Program,
    execution: usize,
    persistent: &mut SessionPersistentState,
    entry: &Name,
    fuel: u64,
    host_mode: HostMode,
    sandbox: &SandboxConfig,
) -> Result<SuspendedTier0, Error> {
    if !program.globals.iter().any(|g| &g.name == entry) {
        return Err(Error::UnknownGlobal(entry.clone()));
    }
    let mut machine = Machine::new(heap, program, execution, Vec::new(), fuel, MainStage::Done);
    machine.host_mode = host_mode;
    machine.configure_sandbox(sandbox);
    machine.load_persistent(persistent);
    if let Err(err) = machine.global(entry, None) {
        machine.save_persistent(persistent);
        let _ = machine.cleanup();
        machine.restore_heap_limit();
        return Err(err);
    }
    let initializing_globals = take_initializing_globals(&machine);
    machine.save_persistent(persistent);
    machine.restore_heap_limit();
    Ok(SuspendedTier0 {
        state: Some(Box::new(machine.into_suspended(
            EvalCompletionPhase::Direct,
            None,
            None,
        ))),
        initializing_globals,
        sandbox_config: sandbox.clone(),
    })
}

/// Advances a [`SuspendedTier0`] by up to `max_steps` VM steps against the
/// current session `program` and `persistent` globals.
pub(crate) fn step_tier0_n(
    suspended: &mut SuspendedTier0,
    heap: &mut Heap,
    program: &Program,
    persistent: &mut SessionPersistentState,
    max_steps: usize,
) -> Tier0StepResult {
    let Some(state) = suspended.state.take() else {
        return Tier0StepResult::Finished(ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnsupportedOperation(
                "no active tier-0 step task",
            )),
            stdout: String::new(),
            stderr: String::new(),
        });
    };
    let (mut machine, phase, rollback_module, prev_alloc_module) =
        Machine::from_suspended(heap, program, None, *state);
    let saved_fuel = machine.fuel;
    machine.configure_sandbox(&suspended.sandbox_config);
    machine.fuel = saved_fuel;
    machine.load_persistent(persistent);
    for name in mem::take(&mut suspended.initializing_globals) {
        machine.globals.entry(name).or_insert(None);
    }
    let fuel_before = machine.fuel;
    let slice_budget = u64::try_from(max_steps).unwrap_or(u64::MAX);
    match machine.run_slice(slice_budget) {
        Ok(SliceExit::Yielded) => {
            let steps_taken =
                usize::try_from(fuel_before.saturating_sub(machine.fuel)).unwrap_or(usize::MAX);
            suspended.initializing_globals = take_initializing_globals(&machine);
            machine.save_persistent(persistent);
            machine.restore_heap_limit();
            suspended.state = Some(Box::new(machine.into_suspended(
                phase,
                rollback_module,
                prev_alloc_module,
            )));
            Tier0StepResult::Running { steps_taken }
        }
        Ok(SliceExit::Completed(exit)) => {
            let outcome = machine.finish_session(Ok(exit), persistent);
            Tier0StepResult::Finished(ExecutionReport {
                outcome,
                stdout: mem::take(&mut machine.stdout),
                stderr: mem::take(&mut machine.stderr),
            })
        }
        Err(err) => {
            let outcome = machine.finish_session(Err(err), persistent);
            Tier0StepResult::Finished(ExecutionReport {
                outcome,
                stdout: mem::take(&mut machine.stdout),
                stderr: mem::take(&mut machine.stderr),
            })
        }
    }
}

/// Releases all heap roots owned by a still-paused [`SuspendedTier0`].
pub(crate) fn abort_tier0_step(
    mut suspended: SuspendedTier0,
    heap: &mut Heap,
    program: &Program,
) -> Result<(), Error> {
    let Some(state) = suspended.state.take() else {
        return Ok(());
    };
    let (mut machine, phase, _rollback_module, _prev_alloc_module) =
        Machine::from_suspended(heap, program, None, *state);
    drop_phase_roots(machine.heap, phase);
    machine.cleanup()
}
