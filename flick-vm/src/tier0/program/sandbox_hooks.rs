// Sandbox capability enforcement, resource limit metering, and LIFO finalizer
// unwinding hooks for `Machine`.

use std::slice;

use crate::sandbox::{
    Capability, CapabilitySet, LimitKind, SandboxConfig, SandboxError, WallDeadline,
    check_program_capabilities, required_capability_for_foreign_call,
};

/// Bounded fuel granted while running LIFO scope finalizers after a resource
/// limit fires, so cleanup finalizers execute while adversarial finalizers
/// remain bounded.
const FINALIZER_GRACE_FUEL: u64 = 16_384;

/// Additional stack-frame headroom granted while running LIFO scope finalizers
/// after `max_stack_depth` or another limit fires.
const FINALIZER_GRACE_STACK: usize = 64;

/// Check wall-clock deadlines every 64 VM steps (cheap bitwise mask).
const WALL_TIME_CHECK_MASK: u64 = 63;

#[derive(Debug, Clone, PartialEq, Eq)]
enum SandboxAbort {
    Limit(LimitKind),
    CapabilityDenied {
        capability: Capability,
        binding: String,
    },
}

impl SandboxAbort {
    fn into_error(self) -> Error {
        match self {
            Self::Limit(kind) => Error::LimitExceeded(kind),
            Self::CapabilityDenied {
                capability,
                binding,
            } => Error::CapabilityDenied {
                capability,
                binding,
            },
        }
    }
}

#[derive(Default)]
enum SavedHeapLimit {
    #[default]
    Unchanged,
    Restore(Option<usize>),
}

struct SandboxRuntime {
    capabilities: CapabilitySet,
    fuel_limited: bool,
    max_stack_depth: Option<usize>,
    wall_deadline: Option<WallDeadline>,
    step_counter: u64,
    abort: Option<SandboxAbort>,
    saved_heap_limit: SavedHeapLimit,
}

impl Default for SandboxRuntime {
    fn default() -> Self {
        Self {
            capabilities: CapabilitySet::all(),
            fuel_limited: false,
            max_stack_depth: None,
            wall_deadline: None,
            step_counter: 0,
            abort: None,
            saved_heap_limit: SavedHeapLimit::Unchanged,
        }
    }
}

fn sandbox_error_to_vm_error(err: SandboxError) -> Error {
    match err {
        SandboxError::CapabilityDenied {
            capability,
            binding,
            ..
        } => Error::CapabilityDenied {
            capability,
            binding,
        },
        other => Error::VerificationFailed(other.to_string()),
    }
}

/// Executes `entry` with `arguments` under an explicit [`SandboxConfig`]
/// (static + dynamic capability checks and fuel/heap/wall-time/stack limits
/// with LIFO scope finalization on abort).
pub fn execute_sandboxed_program_outcome(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    arguments: Vec<Value>,
    host_mode: HostMode,
    sandbox: &SandboxConfig,
) -> ExecutionReport {
    if let Err(err) =
        check_program_capabilities(program, Some(slice::from_ref(entry)), sandbox.capabilities)
    {
        let _ = heap.drop_values(arguments);
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(sandbox_error_to_vm_error(err)),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let arguments = match prepare_arguments(heap, program, entry, arguments) {
        Ok(arguments) => arguments,
        Err(error) => {
            return ExecutionReport {
                outcome: ProgramOutcome::Fault(error),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    let execution = next_execution_id();
    let fuel = sandbox.limits.fuel.unwrap_or(u64::MAX);
    let mut machine = Machine::new(heap, program, execution, arguments, fuel, MainStage::Done);
    machine.host_mode = host_mode;
    machine.configure_sandbox(sandbox);
    let result = machine.global(entry, None).and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

/// Executes `entry` using the `main` protocol under an explicit [`SandboxConfig`].
pub fn execute_sandboxed_main_program_outcome(
    heap: &mut Heap,
    program: &Program,
    entry: &Name,
    host_mode: HostMode,
    sandbox: &SandboxConfig,
) -> ExecutionReport {
    if let Err(err) =
        check_program_capabilities(program, Some(slice::from_ref(entry)), sandbox.capabilities)
    {
        return ExecutionReport {
            outcome: ProgramOutcome::Fault(sandbox_error_to_vm_error(err)),
            stdout: String::new(),
            stderr: String::new(),
        };
    }
    let arguments = match prepare_arguments(heap, program, entry, vec![]) {
        Ok(arguments) => arguments,
        Err(error) => {
            return ExecutionReport {
                outcome: ProgramOutcome::Fault(error),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    let execution = next_execution_id();
    let fuel = sandbox.limits.fuel.unwrap_or(u64::MAX);
    let mut machine = Machine::new(heap, program, execution, arguments, fuel, MainStage::Function);
    machine.host_mode = host_mode;
    machine.configure_sandbox(sandbox);
    let result = machine.global(entry, None).and_then(|()| machine.run());
    let outcome = machine.finish(result);
    ExecutionReport {
        outcome,
        stdout: machine.stdout,
        stderr: machine.stderr,
    }
}

impl Machine<'_, '_> {
    fn configure_sandbox(&mut self, config: &SandboxConfig) {
        self.sandbox.capabilities = config.capabilities;
        self.sandbox.max_stack_depth = config.limits.max_stack_depth;
        self.sandbox.wall_deadline = config.limits.start_wall_deadline();
        self.sandbox.step_counter = 0;
        self.sandbox.abort = None;
        if let Some(fuel) = config.limits.fuel {
            self.fuel = fuel;
            self.sandbox.fuel_limited = true;
        }
        if config.limits.heap_bytes.is_some() {
            let prev = self.heap.byte_limit();
            self.sandbox.saved_heap_limit = SavedHeapLimit::Restore(prev);
            self.heap.set_byte_limit(config.limits.heap_bytes);
        }
    }

    fn restore_heap_limit(&mut self) {
        if let SavedHeapLimit::Restore(prev) =
            mem::replace(&mut self.sandbox.saved_heap_limit, SavedHeapLimit::Unchanged)
        {
            self.heap.set_byte_limit(prev);
        }
    }

    fn check_stack_depth(&self, added_frames: usize) -> Result<(), Error> {
        let Some(limit) = self.sandbox.max_stack_depth else {
            return Ok(());
        };
        let effective = if self.sandbox.abort.is_some() {
            limit.saturating_add(FINALIZER_GRACE_STACK)
        } else {
            limit
        };
        if self.frames.len().saturating_add(added_frames) > effective {
            return Err(Error::LimitExceeded(LimitKind::StackDepth));
        }
        Ok(())
    }

    fn check_foreign_call_capability(&self, spec: &ForeignCallSpec) -> Result<(), Error> {
        let (capability, binding) = required_capability_for_foreign_call(spec);
        if !self.sandbox.capabilities.contains(capability) {
            return Err(Error::CapabilityDenied {
                capability,
                binding,
            });
        }
        Ok(())
    }

    fn check_ffi_capability(&self, binding: &str) -> Result<(), Error> {
        if !self.sandbox.capabilities.contains(Capability::Ffi) {
            return Err(Error::CapabilityDenied {
                capability: Capability::Ffi,
                binding: binding.to_owned(),
            });
        }
        Ok(())
    }

    /// Checks heap byte cap and wall-clock deadline before executing a step.
    /// Returns `Ok(true)` if a limit fired and sandbox unwind was initiated.
    fn check_sandbox_pre_step(&mut self) -> Result<bool, Error> {
        if self.sandbox.abort.is_some() {
            return Ok(false);
        }
        if self.heap.is_byte_limit_exceeded() {
            self.begin_sandbox_unwind(SandboxAbort::Limit(LimitKind::HeapMemory))?;
            return Ok(true);
        }
        if let Some(deadline) = &self.sandbox.wall_deadline {
            let check_now = (self.sandbox.step_counter & WALL_TIME_CHECK_MASK) == 0;
            self.sandbox.step_counter = self.sandbox.step_counter.wrapping_add(1);
            if check_now && deadline.is_expired() {
                self.begin_sandbox_unwind(SandboxAbort::Limit(LimitKind::WallTime))?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Checks whether the just-executed step crossed the heap byte cap.
    fn check_sandbox_post_step(&mut self) -> Result<(), Error> {
        if self.sandbox.abort.is_none() && self.heap.is_byte_limit_exceeded() {
            self.begin_sandbox_unwind(SandboxAbort::Limit(LimitKind::HeapMemory))?;
        }
        Ok(())
    }

    /// Intercepts sandbox-level step errors (`LimitExceeded`, `CapabilityDenied`,
    /// or sandboxed `OutOfFuel`) and initiates a clean LIFO finalizer unwind.
    fn handle_sandbox_step_result(&mut self, res: Result<(), Error>) -> Result<(), Error> {
        let Err(err) = res else {
            return self.check_sandbox_post_step();
        };
        if self.sandbox.abort.is_some() {
            return Err(self
                .sandbox
                .abort
                .take()
                .map_or(err, SandboxAbort::into_error));
        }
        match err {
            Error::LimitExceeded(kind) => self.begin_sandbox_unwind(SandboxAbort::Limit(kind)),
            Error::CapabilityDenied {
                capability,
                binding,
            } => self.begin_sandbox_unwind(SandboxAbort::CapabilityDenied {
                capability,
                binding,
            }),
            Error::OutOfFuel if self.sandbox.fuel_limited => {
                self.begin_sandbox_unwind(SandboxAbort::Limit(LimitKind::Fuel))
            }
            other => Err(other),
        }
    }

    fn begin_sandbox_unwind(&mut self, abort: SandboxAbort) -> Result<(), Error> {
        if self.sandbox.abort.is_some() {
            return Err(abort.into_error());
        }
        self.sandbox.abort = Some(abort);
        // Temporarily lift the heap cap so LIFO scope finalizers can allocate
        // cleanup values/messages; `restore_heap_limit` restores it on exit.
        self.heap.clear_byte_limit_exceeded();
        self.heap.set_byte_limit(None);
        self.fuel = FINALIZER_GRACE_FUEL;
        self.release_operands()?;
        if let Some(pending) = self.pending.take() {
            self.heap.drop_value(pending)?;
        }
        if let Some(handler) = self.pending_handler.take() {
            self.heap.drop_value(handler)?;
        }
        let cause = self
            .heap
            .cause(&[Reason::Interrupt { fiber_id: None }], &mut self.fuel)?;
        if let Some(prev) = self.failure.replace(cause) {
            self.heap.drop_cause(prev)?;
        }
        self.failure_elected = true;
        Ok(())
    }

    /// Uninstalls a handler activation during a sandbox-abort unwind without
    /// invoking its user `errors` recovery arm.
    fn handler_abort_unwind(&mut self, id: usize) -> Result<(), Error> {
        let activation = self.activations.remove(&id).expect("aborted handler");
        self.current_owner = activation.owner;
        let mut roots: Vec<Value> = activation
            .operations
            .into_iter()
            .map(|op| op.function)
            .collect();
        roots.extend(activation.ret);
        roots.extend(activation.errors);
        self.heap.drop_values(roots)
    }
}
