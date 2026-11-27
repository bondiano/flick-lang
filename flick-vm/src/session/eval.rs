//! Phrase, declaration, `:handle`, and `:provide` evaluation on [`VmSession`].

use std::slice;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use flick_core::ir::memory::{Name, Program};

use super::format::{describe_cause, extract_unhandled_effect, is_interrupt_cause, render_value};
use super::{EvalOutcome, HandlerKind, InstalledHandler, InstalledLayer, VmSession};
use crate::sandbox::{SandboxError, check_program_capabilities};
use crate::tier0::{
    Error, ExecutionReport, ProgramOutcome, SessionHandlerKind, SessionRunSpec, Value,
    build_session_layer_value, execute_session_step,
};

impl VmSession {
    pub(super) fn check_link_capabilities(
        &self,
        incoming: &Program,
        roots: &[Name],
    ) -> Option<EvalOutcome> {
        match check_program_capabilities(incoming, Some(roots), self.sandbox.capabilities) {
            Ok(()) => None,
            Err(SandboxError::CapabilityDenied {
                capability,
                binding,
                span,
            }) => Some(EvalOutcome::CapabilityDenied {
                capability,
                binding,
                span,
                stdout: String::new(),
            }),
            Err(other) => Some(EvalOutcome::Fault {
                message: other.to_string(),
                stdout: String::new(),
            }),
        }
    }

    /// Evaluates a phrase expression (`Repl.P<n>.it`), optionally auto-awaiting tasks
    /// under `Host.live`, installed `:provide` layers, and installed `:handle` handlers.
    pub fn eval_phrase(
        &mut self,
        incoming: &Program,
        entry: &Name,
        root: Option<&Name>,
        await_task: bool,
        interrupt: Option<Arc<AtomicBool>>,
    ) -> EvalOutcome {
        if let Some(denied) = self.check_link_capabilities(incoming, slice::from_ref(entry)) {
            return denied;
        }
        self.program.load(incoming);
        if let Some(err_outcome) = self.ensure_root_layer(root, interrupt.clone()) {
            self.rollback_module(&entry.module);
            return err_outcome;
        }
        let report = self.run_global(entry, await_task, interrupt);
        let outcome = self.classify_value_report(report);
        if matches!(
            outcome,
            EvalOutcome::TypedError { .. }
                | EvalOutcome::UnhandledEffect { .. }
                | EvalOutcome::Interrupted { .. }
                | EvalOutcome::CapabilityDenied { .. }
                | EvalOutcome::LimitExceeded { .. }
                | EvalOutcome::Fault { .. }
        ) {
            self.rollback_module(&entry.module);
        }
        outcome
    }

    /// Loads a declaration phrase and eagerly initializes its value globals.
    pub fn eval_declarations(
        &mut self,
        incoming: &Program,
        module: &str,
        entries: &[Name],
        root: Option<&Name>,
        interrupt: Option<&Arc<AtomicBool>>,
    ) -> EvalOutcome {
        let roots: Vec<Name> = if entries.is_empty() {
            incoming
                .globals
                .iter()
                .filter(|g| g.name.module == module)
                .map(|g| g.name.clone())
                .collect()
        } else {
            entries.to_vec()
        };
        if let Some(denied) = self.check_link_capabilities(incoming, &roots) {
            return denied;
        }
        self.program.load(incoming);
        if let Some(err_outcome) = self.ensure_root_layer(root, interrupt.cloned()) {
            self.rollback_module(module);
            return err_outcome;
        }
        let mut stdout = String::new();
        for entry in entries {
            if !self
                .program
                .program()
                .globals
                .iter()
                .any(|g| &g.name == entry)
            {
                continue;
            }
            let report = self.run_global(entry, false, interrupt.cloned());
            stdout.push_str(&report.stdout);
            match report.outcome {
                ProgramOutcome::Returned(value) => {
                    if let Err(err) = self.heap.drop_value(value) {
                        self.rollback_module(module);
                        return EvalOutcome::Fault {
                            message: err.to_string(),
                            stdout,
                        };
                    }
                }
                other => {
                    let outcome = self.classify_non_value_outcome(other, stdout);
                    self.rollback_module(module);
                    return outcome;
                }
            }
        }
        EvalOutcome::Declaration { stdout }
    }

    /// Evaluates and installs a session-wide `:handle` closure wrapping subsequent phrases.
    pub fn install_handler(
        &mut self,
        incoming: &Program,
        entry: &Name,
        root: Option<&Name>,
        kind: HandlerKind,
        handled_effects: Vec<String>,
        interrupt: Option<Arc<AtomicBool>>,
    ) -> EvalOutcome {
        if let Some(denied) = self.check_link_capabilities(incoming, slice::from_ref(entry)) {
            return denied;
        }
        self.program.load(incoming);
        if let Some(err_outcome) = self.ensure_root_layer(root, interrupt.clone()) {
            self.rollback_module(&entry.module);
            return err_outcome;
        }
        let report = self.run_global(entry, false, interrupt);
        let stdout = report.stdout;
        match report.outcome {
            ProgramOutcome::Returned(closure) => {
                self.handlers.push(InstalledHandler {
                    global: entry.clone(),
                    closure,
                    kind,
                    handled_effects,
                });
                EvalOutcome::Declaration { stdout }
            }
            other => {
                self.rollback_module(&entry.module);
                self.classify_non_value_outcome(other, stdout)
            }
        }
    }

    /// Evaluates and installs a session-wide `:provide` layer on top of `Host.live`.
    pub fn install_layer(
        &mut self,
        incoming: &Program,
        entry: &Name,
        root: Option<&Name>,
        provided_services: Vec<String>,
        interrupt: Option<Arc<AtomicBool>>,
    ) -> EvalOutcome {
        if let Some(denied) = self.check_link_capabilities(incoming, slice::from_ref(entry)) {
            return denied;
        }
        self.program.load(incoming);
        if let Some(err_outcome) = self.ensure_root_layer(root, interrupt.clone()) {
            self.rollback_module(&entry.module);
            return err_outcome;
        }
        let provided_reps: Vec<Value> = self
            .layers
            .iter()
            .map(|l| l.value.representation())
            .collect();
        let spec = SessionRunSpec {
            entry,
            await_task: false,
            root_layer: self.root_layer.as_ref(),
            provided_layers: &provided_reps,
            handlers: &[],
            fuel: self.fuel,
            host_mode: self.host_mode,
            program_args: &self.program_args,
            external_interrupt: interrupt,
            synthetic_codes: super::program::SessionProgram::synthetic_codes(),
            sandbox: &self.sandbox,
        };
        let report = build_session_layer_value(
            &mut self.heap,
            self.program.program(),
            self.execution,
            &mut self.persistent,
            &spec,
            self.host_callbacks.as_deref_mut(),
        );
        let stdout = report.stdout;
        match report.outcome {
            ProgramOutcome::Returned(layer_val) => {
                self.layers.push(InstalledLayer {
                    global: entry.clone(),
                    value: layer_val,
                    provided_services,
                });
                EvalOutcome::Declaration { stdout }
            }
            other => {
                self.rollback_module(&entry.module);
                self.classify_non_value_outcome(other, stdout)
            }
        }
    }

    pub(super) fn ensure_root_layer(
        &mut self,
        root: Option<&Name>,
        interrupt: Option<Arc<AtomicBool>>,
    ) -> Option<EvalOutcome> {
        if self.root_layer.is_some() {
            return None;
        }
        let root_name = root?;
        if !self
            .program
            .program()
            .globals
            .iter()
            .any(|g| &g.name == root_name)
        {
            return None;
        }
        let spec = SessionRunSpec {
            entry: root_name,
            await_task: false,
            root_layer: None,
            provided_layers: &[],
            handlers: &[],
            fuel: self.fuel,
            host_mode: self.host_mode,
            program_args: &self.program_args,
            external_interrupt: interrupt,
            synthetic_codes: super::program::SessionProgram::synthetic_codes(),
            sandbox: &self.sandbox,
        };
        let report = execute_session_step(
            &mut self.heap,
            self.program.program(),
            self.execution,
            &mut self.persistent,
            &spec,
            self.host_callbacks.as_deref_mut(),
        );
        match report.outcome {
            ProgramOutcome::Returned(layer) => {
                self.root_layer = Some(layer);
                self.root_layer_name = Some(root_name.clone());
                None
            }
            other => Some(self.classify_non_value_outcome(other, report.stdout)),
        }
    }

    fn run_global(
        &mut self,
        entry: &Name,
        await_task: bool,
        interrupt: Option<Arc<AtomicBool>>,
    ) -> ExecutionReport {
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
            external_interrupt: interrupt,
            synthetic_codes: super::program::SessionProgram::synthetic_codes(),
            sandbox: &self.sandbox,
        };
        let prev_mod = self.heap.set_allocation_module(Some(&entry.module));
        let report = execute_session_step(
            &mut self.heap,
            self.program.program(),
            self.execution,
            &mut self.persistent,
            &spec,
            self.host_callbacks.as_deref_mut(),
        );
        self.heap.restore_allocation_module(prev_mod);
        report
    }

    pub(super) fn classify_value_report(&mut self, report: ExecutionReport) -> EvalOutcome {
        let stdout = report.stdout;
        match report.outcome {
            ProgramOutcome::Returned(value) => {
                let rendered = render_value(&self.heap, &value);
                let drop_res = self.heap.drop_value(value);
                match (rendered, drop_res) {
                    (Ok(rendered), Ok(())) => EvalOutcome::Value { rendered, stdout },
                    (Err(err), _) | (_, Err(err)) => EvalOutcome::Fault {
                        message: err.to_string(),
                        stdout,
                    },
                }
            }
            other => self.classify_non_value_outcome(other, stdout),
        }
    }

    pub(super) fn classify_non_value_outcome(
        &mut self,
        outcome: ProgramOutcome,
        stdout: String,
    ) -> EvalOutcome {
        match outcome {
            ProgramOutcome::Returned(value) => {
                let _ = self.heap.drop_value(value);
                EvalOutcome::Declaration { stdout }
            }
            ProgramOutcome::Raised(value) => {
                let rendered =
                    render_value(&self.heap, &value).unwrap_or_else(|_| "<typed error>".to_owned());
                let _ = self.heap.drop_value(value);
                EvalOutcome::TypedError { rendered, stdout }
            }
            ProgramOutcome::Failed(cause) => {
                if is_interrupt_cause(&self.heap, &cause) {
                    let _ = self.heap.drop_cause(cause);
                    return EvalOutcome::Interrupted { stdout };
                }
                if let Some((effect, op)) = extract_unhandled_effect(&self.heap, &cause) {
                    let _ = self.heap.drop_cause(cause);
                    return EvalOutcome::UnhandledEffect { effect, op, stdout };
                }
                let message = describe_cause(&self.heap, &cause);
                let _ = self.heap.drop_cause(cause);
                EvalOutcome::Fault { message, stdout }
            }
            ProgramOutcome::Fault(Error::MissingHandler { effect, op }) => {
                EvalOutcome::UnhandledEffect { effect, op, stdout }
            }
            ProgramOutcome::Fault(Error::CapabilityDenied {
                capability,
                binding,
            }) => EvalOutcome::CapabilityDenied {
                capability,
                binding,
                span: None,
                stdout,
            },
            ProgramOutcome::Fault(Error::LimitExceeded(kind)) => EvalOutcome::LimitExceeded {
                kind,
                message: format!("sandbox {kind}"),
                stdout,
            },
            ProgramOutcome::Fault(err) => EvalOutcome::Fault {
                message: err.to_string(),
                stdout,
            },
        }
    }

    pub(super) fn rollback_module(&mut self, module: &str) {
        self.program.rollback_module(module);
        let to_remove: Vec<Name> = self
            .persistent
            .globals
            .keys()
            .filter(|name| name.module == module)
            .cloned()
            .collect();
        for name in to_remove {
            if let Some(val) = self.persistent.globals.remove(&name) {
                let _ = self.heap.drop_value(val);
            }
        }
    }
}
