//! Cooperative single-threaded stepping (`start_eval`, `step`, `abandon`) on [`VmSession`].

use std::slice;

use flick_core::ir::memory::{Name, Program};

use super::program::SessionProgram;
use super::{EvalOutcome, VmSession};
use crate::sandbox::LimitKind;
use crate::tier0::{
    Error, ExecutionReport, Heap, SessionHandlerKind, SessionRunSpec, SessionStepStatus,
    SuspendedSessionEval, TestCaseResult, Value, abandon_session_eval,
    execute_tests_program_with_host, start_session_eval, step_session_eval,
};

/// Result of advancing an active cooperative evaluation by one bounded fuel slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    /// The fuel slice completed and more VM work remains; call [`VmSession::step`]
    /// again or [`VmSession::abandon`] to unwind scopes cleanly.
    Yielded { fuel_used: u64 },
    /// The evaluation finished normally or with a language-level outcome
    /// (`Value`, `Declaration`, `TypedError`, `UnhandledEffect`, `Interrupted`,
    /// `CapabilityDenied`).
    Completed(EvalOutcome),
    /// A sandbox resource limit (`fuel`, `heap_bytes`, `wall_time_ms`,
    /// `max_stack_depth`) fired and unwound LIFO finalizers.
    LimitExceeded {
        kind: LimitKind,
        message: String,
        stdout: String,
        stderr: String,
    },
    /// An unrecoverable VM fault stopped evaluation.
    Faulted {
        message: String,
        stdout: String,
        stderr: String,
    },
}

impl VmSession {
    /// Returns `true` while a cooperative evaluation started by
    /// [`Self::start_eval`] or [`Self::start_eval_loaded`] is in progress.
    #[must_use]
    pub const fn has_active_eval(&self) -> bool {
        self.active_eval.is_some()
    }

    /// Returns stdout buffered so far by an active cooperative evaluation.
    #[must_use]
    pub fn active_stdout(&self) -> Option<&str> {
        self.active_eval
            .as_deref()
            .map(SuspendedSessionEval::stdout)
    }

    /// Returns stderr buffered so far by an active cooperative evaluation.
    #[must_use]
    pub fn active_stderr(&self) -> Option<&str> {
        self.active_eval
            .as_deref()
            .map(SuspendedSessionEval::stderr)
    }

    /// Starts a cooperative evaluation of `incoming` phrase `entry` (similar to
    /// [`Self::eval_phrase`]), yielding back to the host via [`Self::step`].
    ///
    /// # Errors
    /// Returns an [`EvalOutcome`] immediately if another evaluation is already
    /// active, if static capability checks deny `entry`, or if root-layer setup fails.
    pub fn start_eval(
        &mut self,
        incoming: &Program,
        entry: &Name,
        root: Option<&Name>,
        await_task: bool,
    ) -> Result<(), EvalOutcome> {
        if self.active_eval.is_some() {
            return Err(EvalOutcome::Fault {
                message: "an evaluation is already in progress; call step() or abandon() first"
                    .to_owned(),
                stdout: String::new(),
            });
        }
        if let Some(denied) = self.check_link_capabilities(incoming, slice::from_ref(entry)) {
            return Err(denied);
        }
        self.program.load(incoming);
        if let Some(err_outcome) = self.ensure_root_layer(root, None) {
            self.rollback_module(&entry.module);
            return Err(err_outcome);
        }
        self.start_eval_inner(entry, await_task, false, Some(entry.module.clone()))
    }

    /// Starts a cooperative evaluation of an already-loaded global (for example
    /// `Main.main` from [`Self::load_image`] / [`Self::load_image_bytes`]).
    ///
    /// When `main_protocol` is `true`, runs the standard Flick `main` apply/await
    /// protocol; when `false`, evaluates the global and optionally auto-awaits a
    /// returned task.
    ///
    /// # Errors
    /// Returns an [`EvalOutcome`] immediately if another evaluation is already
    /// active, if static capability checks deny `entry`, or if root-layer setup fails.
    pub fn start_eval_loaded(
        &mut self,
        entry: &Name,
        main_protocol: bool,
    ) -> Result<(), EvalOutcome> {
        if self.active_eval.is_some() {
            return Err(EvalOutcome::Fault {
                message: "an evaluation is already in progress; call step() or abandon() first"
                    .to_owned(),
                stdout: String::new(),
            });
        }
        if let Some(denied) =
            self.check_link_capabilities(self.program.program(), slice::from_ref(entry))
        {
            return Err(denied);
        }
        let root_name = self.root_layer_name.clone();
        if let Some(err_outcome) = self.ensure_root_layer(root_name.as_ref(), None) {
            return Err(err_outcome);
        }
        self.start_eval_inner(entry, !main_protocol, main_protocol, None)
    }

    fn start_eval_inner(
        &mut self,
        entry: &Name,
        await_task: bool,
        main_protocol: bool,
        rollback_module: Option<String>,
    ) -> Result<(), EvalOutcome> {
        let provided_reps: Vec<Value> = self
            .layers
            .iter()
            .map(|l| l.value.representation())
            .collect();
        let handler_reps: Vec<(Value, SessionHandlerKind)> = self
            .handlers
            .iter()
            .map(|h| (h.closure.representation(), h.kind.into()))
            .collect();
        let spec = SessionRunSpec {
            entry,
            await_task,
            root_layer: self.root_layer.as_ref(),
            provided_layers: &provided_reps,
            handlers: &handler_reps,
            fuel: self.fuel,
            host_mode: self.host_mode,
            program_args: &self.program_args,
            external_interrupt: None,
            synthetic_codes: SessionProgram::synthetic_codes(),
            sandbox: &self.sandbox,
        };
        match start_session_eval(
            &mut self.heap,
            self.program.program(),
            self.execution,
            &mut self.persistent,
            &spec,
            main_protocol,
            rollback_module.clone(),
            self.host_callbacks.as_deref_mut(),
        ) {
            Ok(suspended) => {
                self.active_eval = Some(suspended);
                Ok(())
            }
            Err(report) => {
                let outcome = self.classify_value_report(*report);
                if let Some(module) = rollback_module {
                    self.rollback_module(&module);
                }
                Err(outcome)
            }
        }
    }

    /// Runs the active cooperative evaluation for up to `fuel` VM steps.
    pub fn step(&mut self, fuel: u64) -> StepOutcome {
        let rollback_module = self
            .active_eval
            .as_ref()
            .and_then(|s| s.rollback_module.clone());
        match step_session_eval(
            &mut self.heap,
            self.program.program(),
            &mut self.persistent,
            &mut self.active_eval,
            fuel,
            self.host_callbacks.as_deref_mut(),
        ) {
            SessionStepStatus::Yielded { fuel_used } => StepOutcome::Yielded { fuel_used },
            SessionStepStatus::Finished(report) => {
                self.finish_step_report(report, rollback_module.as_deref())
            }
        }
    }

    /// Abandons the active cooperative evaluation, unwinding open scopes and
    /// running LIFO finalizers cleanly without corrupting the session or
    /// leaking heap slots.
    pub fn abandon(&mut self) -> EvalOutcome {
        let Some(state) = self.active_eval.take() else {
            return EvalOutcome::Interrupted {
                stdout: String::new(),
            };
        };
        let rollback_module = state.rollback_module.clone();
        let report = abandon_session_eval(
            &mut self.heap,
            self.program.program(),
            &mut self.persistent,
            *state,
            self.host_callbacks.as_deref_mut(),
        );
        let outcome = self.classify_value_report(report);
        if let Some(module) = rollback_module {
            self.rollback_module(&module);
        }
        match outcome {
            EvalOutcome::Interrupted { stdout }
            | EvalOutcome::Fault { stdout, .. }
            | EvalOutcome::LimitExceeded { stdout, .. }
            | EvalOutcome::Value { stdout, .. }
            | EvalOutcome::Declaration { stdout }
            | EvalOutcome::TypedError { stdout, .. }
            | EvalOutcome::UnhandledEffect { stdout, .. }
            | EvalOutcome::CapabilityDenied { stdout, .. } => EvalOutcome::Interrupted { stdout },
        }
    }

    fn finish_step_report(
        &mut self,
        report: ExecutionReport,
        rollback_module: Option<&str>,
    ) -> StepOutcome {
        let stderr = report.stderr.clone();
        let outcome = self.classify_value_report(report);
        let failed = matches!(
            outcome,
            EvalOutcome::TypedError { .. }
                | EvalOutcome::UnhandledEffect { .. }
                | EvalOutcome::Interrupted { .. }
                | EvalOutcome::CapabilityDenied { .. }
                | EvalOutcome::LimitExceeded { .. }
                | EvalOutcome::Fault { .. }
        );
        if failed && let Some(module) = rollback_module {
            self.rollback_module(module);
        }
        match outcome {
            EvalOutcome::LimitExceeded {
                kind,
                message,
                stdout,
            } => StepOutcome::LimitExceeded {
                kind,
                message,
                stdout,
                stderr,
            },
            EvalOutcome::Fault { message, stdout } => StepOutcome::Faulted {
                message,
                stdout,
                stderr,
            },
            other => StepOutcome::Completed(other),
        }
    }

    /// Runs a loaded `Test` suite global (e.g. `Language.Arithmetic.tests`)
    /// from the session's linked program on an isolated counted heap and
    /// verifies zero leaked heap slots.
    pub fn run_loaded_test_suite(
        &self,
        entry: &Name,
        seed: i32,
        fuel_per_test: u64,
        skip_groups: &[String],
    ) -> Result<Vec<TestCaseResult>, Error> {
        let mut heap = Heap::new();
        let skip_refs: Vec<&str> = skip_groups.iter().map(String::as_str).collect();
        let results = execute_tests_program_with_host(
            &mut heap,
            self.program.program(),
            entry,
            self.root_layer_name.as_ref(),
            seed,
            fuel_per_test,
            &skip_refs,
            self.host_mode,
        );
        match results {
            Ok(cases) => {
                heap.finish()?;
                Ok(cases)
            }
            Err(err) => {
                let _ = heap.finish();
                Err(err)
            }
        }
    }
}
