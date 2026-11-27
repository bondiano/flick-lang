// Cooperative single-threaded stepping entrypoints (`start_session_eval`,
// `step_session_eval`, `abandon_session_eval`) for `crate::session`.

/// Result of advancing a suspended session evaluation by one fuel slice.
pub(crate) enum SessionStepStatus {
    Yielded { fuel_used: u64 },
    Finished(ExecutionReport),
}

/// Initializes a cooperative session evaluation ready for bounded [`step_session_eval`] calls.
#[expect(clippy::too_many_arguments, reason = "cooperative session start parameters")]
pub(crate) fn start_session_eval<'h>(
    heap: &'h mut Heap,
    program: &Program,
    execution: usize,
    persistent: &mut SessionPersistentState,
    spec: &SessionRunSpec<'_>,
    main_protocol: bool,
    rollback_module: Option<String>,
    host_callbacks: Option<&'h mut (dyn HostCallbacks + 'static)>,
) -> Result<Box<SuspendedSessionEval>, Box<ExecutionReport>> {
    if !program.globals.iter().any(|g| &g.name == spec.entry) {
        return Err(Box::new(ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(spec.entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        }));
    }
    let prev_alloc_module = heap.set_allocation_module(Some(&spec.entry.module));
    let main_stage = if main_protocol {
        MainStage::Function
    } else {
        MainStage::Done
    };
    let mut machine = Machine::new(heap, program, execution, vec![], spec.fuel, main_stage);
    machine.host_mode = spec.host_mode;
    machine.program_args = spec.program_args.to_vec();
    machine.host_callbacks = host_callbacks;
    machine.external_interrupt.clone_from(&spec.external_interrupt);
    machine.configure_sandbox(spec.sandbox);
    machine.load_persistent(persistent);

    let setup_res = machine
        .install_session_context(spec.root_layer, spec.provided_layers)
        .and_then(|()| machine.global(spec.entry, None));

    if let Err(err) = setup_res {
        let outcome = machine.finish_session(Err(err), persistent);
        machine.heap.restore_allocation_module(prev_alloc_module);
        return Err(Box::new(ExecutionReport {
            outcome,
            stdout: machine.stdout,
            stderr: machine.stderr,
        }));
    }

    let phase = if !main_protocol && spec.await_task {
        let mut owned_handlers = Vec::with_capacity(spec.handlers.len());
        for (rep, kind) in spec.handlers {
            match machine.heap.dup(rep) {
                Ok(v) => owned_handlers.push((v, *kind)),
                Err(err) => {
                    let _ = machine
                        .heap
                        .drop_values(owned_handlers.into_iter().map(|(v, _)| v).collect());
                    let outcome = machine.finish_session(Err(err), persistent);
                    machine.heap.restore_allocation_module(prev_alloc_module);
                    return Err(Box::new(ExecutionReport {
                        outcome,
                        stdout: machine.stdout,
                        stderr: machine.stderr,
                    }));
                }
            }
        }
        EvalCompletionPhase::PhraseAwait {
            handlers: owned_handlers,
            synthetic_codes: spec.synthetic_codes,
            awaiting: false,
        }
    } else {
        EvalCompletionPhase::Direct
    };

    Ok(Box::new(machine.into_suspended(
        phase,
        rollback_module,
        prev_alloc_module,
    )))
}

/// Advances a suspended session evaluation by at most `slice_fuel` VM steps.
pub(crate) fn step_session_eval<'h>(
    heap: &'h mut Heap,
    program: &Program,
    persistent: &mut SessionPersistentState,
    suspended: &mut Option<Box<SuspendedSessionEval>>,
    slice_fuel: u64,
    host_callbacks: Option<&'h mut (dyn HostCallbacks + 'static)>,
) -> SessionStepStatus {
    let Some(state) = suspended.take() else {
        return SessionStepStatus::Finished(ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnsupportedOperation(
                "no active evaluation to step",
            )),
            stdout: String::new(),
            stderr: String::new(),
        });
    };
    let (mut machine, mut phase, rollback_module, prev_alloc_module) =
        Machine::from_suspended(heap, program, host_callbacks, *state);
    let fuel_before = machine.fuel;
    let slice_res = machine.run_slice(slice_fuel);
    let fuel_used = fuel_before.saturating_sub(machine.fuel);

    let finished_result: Option<Result<LanguageExit, Error>> = match slice_res {
        Ok(SliceExit::Yielded) => {
            *suspended = Some(Box::new(machine.into_suspended(
                phase,
                rollback_module,
                prev_alloc_module,
            )));
            return SessionStepStatus::Yielded { fuel_used };
        }
        Ok(SliceExit::Completed(exit)) => match &mut phase {
            EvalCompletionPhase::PhraseAwait {
                handlers,
                synthetic_codes,
                awaiting,
            } if !*awaiting => match exit {
                LanguageExit::Success(val) => match machine.is_awaitable_task(&val) {
                    Ok(true) => {
                        *awaiting = true;
                        let handler_reps: Vec<(Value, SessionHandlerKind)> = handlers
                            .iter()
                            .map(|(v, k)| (v.representation(), *k))
                            .collect();
                        let codes = *synthetic_codes;
                        let start_await = machine
                            .wrap_task_with_handlers(val, &handler_reps, codes)
                            .and_then(|wrapped| {
                                machine.scratch.extend([wrapped, Value::Unit]);
                                machine.await_task_fresh(Destination::Top)
                            });
                        match start_await {
                            Ok(()) => {
                                let remaining = slice_fuel.saturating_sub(fuel_used);
                                if remaining == 0 {
                                    *suspended = Some(Box::new(machine.into_suspended(
                                        phase,
                                        rollback_module,
                                        prev_alloc_module,
                                    )));
                                    return SessionStepStatus::Yielded { fuel_used };
                                }
                                let before2 = machine.fuel;
                                match machine.run_slice(remaining) {
                                    Ok(SliceExit::Yielded) => {
                                        let used2 = before2.saturating_sub(machine.fuel);
                                        *suspended = Some(Box::new(machine.into_suspended(
                                            phase,
                                            rollback_module,
                                            prev_alloc_module,
                                        )));
                                        return SessionStepStatus::Yielded {
                                            fuel_used: fuel_used.saturating_add(used2),
                                        };
                                    }
                                    Ok(SliceExit::Completed(exit2)) => Some(Ok(exit2)),
                                    Err(err) => Some(Err(err)),
                                }
                            }
                            Err(err) => Some(Err(err)),
                        }
                    }
                    Ok(false) => Some(Ok(LanguageExit::Success(val))),
                    Err(err) => {
                        let _ = machine.heap.drop_value(val);
                        Some(Err(err))
                    }
                },
                LanguageExit::Failure(cause) => Some(Ok(LanguageExit::Failure(cause))),
            },
            _ => Some(Ok(exit)),
        },
        Err(err) => Some(Err(err)),
    };

    drop_phase_roots(machine.heap, phase);
    let outcome = machine.finish_session(finished_result.expect("finished result"), persistent);
    machine.heap.restore_allocation_module(prev_alloc_module);
    SessionStepStatus::Finished(ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    })
}

/// Abandons an active suspended session evaluation, running LIFO scope
/// finalizers under bounded grace fuel and releasing all execution roots cleanly.
pub(crate) fn abandon_session_eval<'h>(
    heap: &'h mut Heap,
    program: &Program,
    persistent: &mut SessionPersistentState,
    state: SuspendedSessionEval,
    host_callbacks: Option<&'h mut (dyn HostCallbacks + 'static)>,
) -> ExecutionReport {
    let (mut machine, phase, _rollback_module, prev_alloc_module) =
        Machine::from_suspended(heap, program, host_callbacks, state);
    drop_phase_roots(machine.heap, phase);
    let unwind_res = machine
        .begin_sandbox_unwind(SandboxAbort::Abandoned)
        .and_then(|()| machine.run());
    let outcome = machine.finish_session(unwind_res, persistent);
    machine.heap.restore_allocation_module(prev_alloc_module);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

fn drop_phase_roots(heap: &mut Heap, phase: EvalCompletionPhase) {
    if let EvalCompletionPhase::PhraseAwait { handlers, .. } = phase {
        let vals: Vec<Value> = handlers.into_iter().map(|(v, _)| v).collect();
        let _ = heap.drop_values(vals);
    }
}
