// Persistent-session execution hooks over `Machine` for `crate::session`.

/// Calling convention of a session-installed `:handle` closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionHandlerKind {
    Thunk { returns_task: bool },
    Task { returns_task: bool },
}

/// Reserved synthetic `CodeId`s installed in every session `Program`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SessionSyntheticCodes {
    pub(crate) await_task: CodeId,
    pub(crate) apply: CodeId,
    pub(crate) apply_and_await: CodeId,
}

/// Long-lived heap roots owned by a [`crate::session::VmSession`] across phrases.
#[derive(Default)]
pub(crate) struct SessionPersistentState {
    pub(crate) globals: BTreeMap<Name, Value>,
    pub(crate) cells: Vec<Value>,
    pub(crate) lazies: Vec<Value>,
}

/// Parameters for one phrase/command evaluation step on a persistent session.
pub(crate) struct SessionRunSpec<'a> {
    pub(crate) entry: &'a Name,
    pub(crate) await_task: bool,
    pub(crate) root_layer: Option<&'a Value>,
    pub(crate) provided_layers: &'a [Value],
    pub(crate) handlers: &'a [(Value, SessionHandlerKind)],
    pub(crate) fuel: u64,
    pub(crate) host_mode: HostMode,
    pub(crate) external_interrupt: Option<Arc<AtomicBool>>,
    pub(crate) synthetic_codes: SessionSyntheticCodes,
}

/// Allocates a unique execution id shared by all phrases in one live session.
pub(crate) fn allocate_execution_id() -> usize {
    NEXT_EXECUTION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("fewer than usize::MAX model executions")
}

/// Executes one global (and optionally auto-awaits its task under installed
/// session layers and handlers) against persistent session roots.
pub(crate) fn execute_session_step(
    heap: &mut Heap,
    program: &Program,
    execution: usize,
    persistent: &mut SessionPersistentState,
    spec: &SessionRunSpec<'_>,
) -> ExecutionReport {
    if !program.globals.iter().any(|g| &g.name == spec.entry) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(spec.entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let mut machine = Machine::new(heap, program, execution, vec![], spec.fuel, MainStage::Done);
    machine.host_mode = spec.host_mode;
    machine.external_interrupt.clone_from(&spec.external_interrupt);
    machine.load_persistent(persistent);
    let setup_res = machine.install_session_context(spec.root_layer, spec.provided_layers);
    let result = setup_res.and_then(|()| {
        machine.global(spec.entry, None)?;
        let exit = machine.run()?;
        if !spec.await_task {
            return Ok(exit);
        }
        match exit {
            LanguageExit::Success(val) => {
                if machine.is_awaitable_task(&val)? {
                    let wrapped =
                        machine.wrap_task_with_handlers(val, spec.handlers, spec.synthetic_codes)?;
                    machine.scratch.extend([wrapped, Value::Unit]);
                    machine.await_task_fresh(Destination::Top)?;
                    machine.run()
                } else {
                    Ok(LanguageExit::Success(val))
                }
            }
            LanguageExit::Failure(cause) => Ok(LanguageExit::Failure(cause)),
        }
    });
    let outcome = machine.finish_session(result, persistent);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

/// Evaluates a layer global; static layers are returned as-is while dynamic
/// layers are built into a `Data::Context` once under the active session layers.
pub(crate) fn build_session_layer_value(
    heap: &mut Heap,
    program: &Program,
    execution: usize,
    persistent: &mut SessionPersistentState,
    spec: &SessionRunSpec<'_>,
) -> ExecutionReport {
    if !program.globals.iter().any(|g| &g.name == spec.entry) {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(Error::UnknownGlobal(spec.entry.clone())),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let mut machine = Machine::new(heap, program, execution, vec![], spec.fuel, MainStage::Done);
    machine.host_mode = spec.host_mode;
    machine.external_interrupt.clone_from(&spec.external_interrupt);
    machine.load_persistent(persistent);
    let setup_res = machine.install_session_context(spec.root_layer, spec.provided_layers);
    let result = setup_res.and_then(|()| {
        machine.global(spec.entry, None)?;
        match machine.run()? {
            LanguageExit::Success(layer) => {
                stdlib::runtime::layers::ensure_layer(machine.heap, &layer)?;
                if machine.layer_is_static(&layer)? {
                    Ok(LanguageExit::Success(layer))
                } else {
                    let memo_id = machine.alloc_memo_map(None);
                    let owner = machine.current_owner;
                    machine.begin_build_layer(
                        layer,
                        memo_id,
                        owner,
                        false,
                        LayerBuildTarget::Top,
                    )?;
                    machine.run()
                }
            }
            LanguageExit::Failure(cause) => Ok(LanguageExit::Failure(cause)),
        }
    });
    let outcome = machine.finish_session(result, persistent);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

/// Releases all persistent globals, cells, and lazies cleanly without sweeping.
pub(crate) fn shutdown_persistent_state(
    heap: &mut Heap,
    persistent: &mut SessionPersistentState,
) -> Result<(), Error> {
    let mut roots: Vec<Value> = mem::take(&mut persistent.globals).into_values().collect();
    let mut detached = Vec::new();
    let mut failure = None;
    for root in &persistent.cells {
        let Value::Object(token) = root else {
            failure.get_or_insert(Error::ExpectedCell);
            continue;
        };
        match heap.object_mut(token.id).map(|obj| &mut obj.data) {
            Ok(Data::Cell { contents, .. }) => {
                if let Some(value) = contents.take() {
                    detached.push(value);
                }
            }
            Ok(_) => {
                failure.get_or_insert(Error::ExpectedCell);
            }
            Err(err) => {
                failure.get_or_insert(err);
            }
        }
    }
    for root in &persistent.lazies {
        let Value::Object(token) = root else {
            failure.get_or_insert(Error::ExpectedLazy);
            continue;
        };
        match heap.object_mut(token.id).map(|obj| &mut obj.data) {
            Ok(Data::Lazy { state, .. }) => {
                if let Some(prev) = state.take() {
                    match prev {
                        LazyState::Pending(v)
                        | LazyState::Running { task: v, .. }
                        | LazyState::RunningLocal { task: Some(v) }
                        | LazyState::DoneOk(v) => detached.push(v),
                        LazyState::DoneCause(c) => detached.push(c.into_value()),
                        LazyState::RunningLocal { task: None } => {}
                    }
                }
            }
            Ok(_) => {
                failure.get_or_insert(Error::ExpectedLazy);
            }
            Err(err) => {
                failure.get_or_insert(err);
            }
        }
    }
    roots.extend(mem::take(&mut persistent.cells));
    roots.extend(mem::take(&mut persistent.lazies));
    roots.extend(detached);
    let dropped = heap.drop_values(roots);
    failure.map_or(dropped, Err)
}

impl Machine<'_, '_> {
    fn load_persistent(&mut self, persistent: &mut SessionPersistentState) {
        for (name, val) in mem::take(&mut persistent.globals) {
            self.globals.insert(name, Some(val));
        }
        self.cells = mem::take(&mut persistent.cells);
        self.lazies = mem::take(&mut persistent.lazies);
    }

    fn save_persistent(&mut self, persistent: &mut SessionPersistentState) {
        for (name, opt) in mem::take(&mut self.globals) {
            if let Some(val) = opt {
                persistent.globals.insert(name, val);
            }
        }
        persistent.cells = mem::take(&mut self.cells);
        persistent.lazies = mem::take(&mut self.lazies);
    }

    fn install_session_context(
        &mut self,
        root_layer: Option<&Value>,
        provided_layers: &[Value],
    ) -> Result<(), Error> {
        if let Some(root) = root_layer {
            let dup = self.heap.dup(root)?;
            let id = self.handler_identity()?;
            let owner = self.current_owner;
            self.providers.insert(
                id,
                Provider {
                    layer: dup,
                    owner,
                    previous: owner,
                    restore_memo: MemoRestore::Unchanged,
                },
            );
            self.root = Some(id);
        }
        // Newest `:provide` layer is checked first, overriding earlier ones and `Host.live`.
        for layer in provided_layers.iter().rev() {
            let dup = self.heap.dup(layer)?;
            self.inherited_services.push(dup);
        }
        Ok(())
    }

    fn is_awaitable_task(&self, val: &Value) -> Result<bool, Error> {
        let Value::Object(token) = val else {
            return Ok(false);
        };
        Ok(matches!(
            self.heap.object(token.id)?.data,
            Data::Task { .. }
                | Data::MaskedTask { .. }
                | Data::CachedRunTask { .. }
                | Data::CachedInvalidateTask { .. }
                | Data::ExitTask(_)
                | Data::ForeignHandle { .. }
        ))
    }

    fn wrap_task_with_handlers(
        &mut self,
        mut task: Value,
        handlers: &[(Value, SessionHandlerKind)],
        codes: SessionSyntheticCodes,
    ) -> Result<Value, Error> {
        let execution = self.execution;
        // `handlers` is ordered oldest -> newest; wrapping from newest to oldest
        // makes the newest `:handle` innermost so it takes precedence.
        for (handler_rep, kind) in handlers.iter().rev() {
            let handler_fn = match self.heap.dup(handler_rep) {
                Ok(v) => v,
                Err(err) => {
                    let _ = self.heap.drop_value(task);
                    return Err(err);
                }
            };
            task = match *kind {
                SessionHandlerKind::Thunk { returns_task } => {
                    let thunk = self.heap.allocate(Data::Closure {
                        execution,
                        code: codes.await_task,
                        captures: vec![task],
                    });
                    let code = if returns_task {
                        codes.apply_and_await
                    } else {
                        codes.apply
                    };
                    self.heap.allocate(Data::Task {
                        execution,
                        code,
                        captures: vec![handler_fn, thunk],
                    })
                }
                SessionHandlerKind::Task { returns_task } => {
                    let code = if returns_task {
                        codes.apply_and_await
                    } else {
                        codes.apply
                    };
                    self.heap.allocate(Data::Task {
                        execution,
                        code,
                        captures: vec![handler_fn, task],
                    })
                }
            };
        }
        Ok(task)
    }

    fn finish_session(
        &mut self,
        result: Result<LanguageExit, Error>,
        persistent: &mut SessionPersistentState,
    ) -> ProgramOutcome {
        let outcome = match result.and_then(|exit| self.report_exit(exit)) {
            Ok(outcome) => outcome,
            Err(error) => ProgramOutcome::Fault(error),
        };
        self.save_persistent(persistent);
        if let Err(error) = self.cleanup() {
            let _ = outcome.release(self.heap);
            return ProgramOutcome::Fault(error);
        }
        outcome
    }
}
