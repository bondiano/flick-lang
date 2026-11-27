//! In-process VM [`ReplEngine`] backed by `flick_core::repl::Session` and
//! `flick_vm::session::VmSession`.

use std::slice;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use flick_core::diagnostics::{Diagnostic, render as renderers};
use flick_core::iface::Namespace;
use flick_core::ir::memory::Name as MemoryName;
use flick_core::repl::{HandlerStyle, Session};
use flick_core::types::{Type, show_type};
use flick_vm::session::{EvalOutcome, HandlerKind, VmSession};

use super::engine::{EngineResponse, ReplEngine};
use crate::vm::lower;

/// Live VM REPL engine combining incremental checking with persistent VM execution.
pub struct VmReplEngine {
    session: Session,
    vm: VmSession,
}

impl Default for VmReplEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl VmReplEngine {
    pub fn new() -> Self {
        Self {
            session: Session::with_target("vm"),
            vm: VmSession::new(),
        }
    }

    fn render_diagnostics(&self, diagnostics: &[Diagnostic]) -> String {
        renderers::render_human(self.session.sources(), diagnostics, false)
            .trim_end()
            .to_owned()
    }
}

impl ReplEngine for VmReplEngine {
    fn eval(&mut self, input: &str, interrupt: Arc<AtomicBool>) -> EngineResponse {
        let accepted = match self.session.enter(input) {
            Ok(accepted) => accepted,
            Err(diagnostics) => {
                return EngineResponse::Error {
                    stdout: String::new(),
                    message: self.render_diagnostics(&diagnostics),
                };
            }
        };

        let module = accepted.phrase.module.clone();
        if accepted.expression {
            let entry = MemoryName::new(&module, "it");
            let lowered = match lower::lower(&accepted.compilation, slice::from_ref(&entry), false)
            {
                Ok(lowered) => lowered,
                Err(err) => {
                    self.session.rollback_last();
                    return EngineResponse::Error {
                        stdout: String::new(),
                        message: format!("VM lowering failed: {err}"),
                    };
                }
            };
            let is_task = accepted.is_task;
            let value_type = accepted.value_type.clone();
            let ok_type = accepted.ok_type.clone();
            let err_type = accepted.err_type.clone();
            let outcome = self.vm.eval_phrase(
                &lowered.program,
                &entry,
                lowered.root.as_ref(),
                is_task,
                Some(interrupt),
            );
            self.finish_expression_outcome(outcome, is_task, value_type, ok_type, err_type)
        } else {
            let entries: Vec<MemoryName> = accepted
                .phrase
                .names
                .iter()
                .filter(|e| e.entity.module == module && e.entity.namespace == Namespace::Value)
                .map(|e| MemoryName::new(&module, &e.entity.name))
                .collect();
            if entries.is_empty() {
                return EngineResponse::Output {
                    stdout: String::new(),
                    result: None,
                };
            }
            let lowered = match lower::lower(&accepted.compilation, &entries, false) {
                Ok(lowered) => lowered,
                Err(err) => {
                    self.session.rollback_last();
                    return EngineResponse::Error {
                        stdout: String::new(),
                        message: format!("VM lowering failed: {err}"),
                    };
                }
            };
            let outcome = self.vm.eval_declarations(
                &lowered.program,
                &module,
                &entries,
                lowered.root.as_ref(),
                Some(&interrupt),
            );
            self.finish_declaration_outcome(outcome)
        }
    }

    fn handle(&mut self, expr: &str, interrupt: Arc<AtomicBool>) -> EngineResponse {
        let accepted = match self.session.enter_handle(expr) {
            Ok(accepted) => accepted,
            Err(diagnostics) => {
                return EngineResponse::Error {
                    stdout: String::new(),
                    message: self.render_diagnostics(&diagnostics),
                };
            }
        };
        let entry = MemoryName::new(&accepted.phrase.module, &accepted.global_name);
        let handled_effects = accepted.handled_effects.clone();
        let kind = match accepted.style {
            HandlerStyle::Thunk { returns_task } => HandlerKind::Thunk { returns_task },
            HandlerStyle::Task { returns_task } => HandlerKind::Task { returns_task },
        };
        let lowered = match lower::lower(&accepted.compilation, slice::from_ref(&entry), false) {
            Ok(lowered) => lowered,
            Err(err) => {
                self.session.rollback_last();
                return EngineResponse::Error {
                    stdout: String::new(),
                    message: format!("VM lowering failed: {err}"),
                };
            }
        };
        let outcome = self.vm.install_handler(
            &lowered.program,
            &entry,
            lowered.root.as_ref(),
            kind,
            handled_effects.clone(),
            Some(interrupt),
        );
        match outcome {
            EvalOutcome::Declaration { stdout } => {
                let names: Vec<&str> = handled_effects
                    .iter()
                    .map(|e| e.rsplit('.').next().unwrap_or(e.as_str()))
                    .collect();
                let label = if names.is_empty() {
                    "installed handler".to_owned()
                } else {
                    format!("handling {}", names.join(", "))
                };
                EngineResponse::Output {
                    stdout,
                    result: Some(label),
                }
            }
            other => {
                self.session.rollback_last();
                Self::map_error_outcome(other)
            }
        }
    }

    fn provide(&mut self, expr: &str, interrupt: Arc<AtomicBool>) -> EngineResponse {
        let accepted = match self.session.enter_provide(expr) {
            Ok(accepted) => accepted,
            Err(diagnostics) => {
                return EngineResponse::Error {
                    stdout: String::new(),
                    message: self.render_diagnostics(&diagnostics),
                };
            }
        };
        let entry = MemoryName::new(&accepted.phrase.module, &accepted.global_name);
        let provided_services = accepted.provided_services.clone();
        let lowered = match lower::lower(&accepted.compilation, slice::from_ref(&entry), false) {
            Ok(lowered) => lowered,
            Err(err) => {
                self.session.rollback_last();
                return EngineResponse::Error {
                    stdout: String::new(),
                    message: format!("VM lowering failed: {err}"),
                };
            }
        };
        let outcome = self.vm.install_layer(
            &lowered.program,
            &entry,
            lowered.root.as_ref(),
            provided_services.clone(),
            Some(interrupt),
        );
        match outcome {
            EvalOutcome::Declaration { stdout } => {
                let names: Vec<&str> = provided_services
                    .iter()
                    .map(|s| s.rsplit('.').next().unwrap_or(s.as_str()))
                    .collect();
                let label = if names.is_empty() {
                    "provided layer".to_owned()
                } else {
                    format!("provided {}", names.join(", "))
                };
                EngineResponse::Output {
                    stdout,
                    result: Some(label),
                }
            }
            other => {
                self.session.rollback_last();
                Self::map_error_outcome(other)
            }
        }
    }

    fn spawn_bg(&mut self, expr: &str) -> EngineResponse {
        let accepted = match self.session.enter_bg(expr) {
            Ok(accepted) => accepted,
            Err(diagnostics) => {
                return EngineResponse::Error {
                    stdout: String::new(),
                    message: self.render_diagnostics(&diagnostics),
                };
            }
        };
        let entry = MemoryName::new(&accepted.phrase.module, &accepted.global_name);
        let lowered = match lower::lower(&accepted.compilation, slice::from_ref(&entry), false) {
            Ok(lowered) => lowered,
            Err(err) => {
                self.session.rollback_last();
                return EngineResponse::Error {
                    stdout: String::new(),
                    message: format!("VM lowering failed: {err}"),
                };
            }
        };
        let summary = expr.lines().next().unwrap_or(expr).trim().to_owned();
        match self
            .vm
            .spawn_bg(&lowered.program, &entry, lowered.root.as_ref(), summary)
        {
            Ok(id) => EngineResponse::Output {
                stdout: String::new(),
                result: Some(format!("[{id}] started")),
            },
            Err(message) => {
                self.session.rollback_last();
                EngineResponse::Error {
                    stdout: String::new(),
                    message,
                }
            }
        }
    }

    fn list_jobs(&self) -> Vec<String> {
        self.vm
            .list_jobs()
            .into_iter()
            .map(|job| format!("[{}] {} {}", job.id, job.status.label(), job.label))
            .collect()
    }

    fn kill_job(&mut self, id: u64) -> EngineResponse {
        match self.vm.kill_job(id) {
            Ok(outcome) => {
                if let Some(leak) = outcome.leak_error {
                    return EngineResponse::Error {
                        stdout: outcome.stdout,
                        message: format!("job {id} teardown leak: {leak}"),
                    };
                }
                EngineResponse::Output {
                    stdout: outcome.stdout,
                    result: Some(format!("[{}] {}", outcome.id, outcome.status.label())),
                }
            }
            Err(message) => EngineResponse::Error {
                stdout: String::new(),
                message,
            },
        }
    }

    fn shutdown(&mut self) -> Result<(), String> {
        self.vm.shutdown()
    }
}

impl VmReplEngine {
    fn finish_expression_outcome(
        &mut self,
        outcome: EvalOutcome,
        is_task: bool,
        value_type: Option<Type>,
        ok_type: Option<Type>,
        err_type: Option<Type>,
    ) -> EngineResponse {
        match outcome {
            EvalOutcome::Value { rendered, stdout } => {
                let display_ty = if is_task {
                    ok_type.or(value_type)
                } else {
                    value_type
                };
                let result = match display_ty {
                    Some(ty) => format!("{rendered} : {}", show_type(&ty)),
                    None => rendered,
                };
                EngineResponse::Output {
                    stdout,
                    result: Some(result),
                }
            }
            EvalOutcome::TypedError { rendered, stdout } => {
                self.session.rollback_last();
                let message = match err_type {
                    Some(ty) => format!("{rendered} : {}", show_type(&ty)),
                    None => rendered,
                };
                EngineResponse::TypedError { stdout, message }
            }
            other => {
                self.session.rollback_last();
                Self::map_error_outcome(other)
            }
        }
    }

    fn finish_declaration_outcome(&mut self, outcome: EvalOutcome) -> EngineResponse {
        match outcome {
            EvalOutcome::Declaration { stdout } => EngineResponse::Output {
                stdout,
                result: None,
            },
            other => {
                self.session.rollback_last();
                Self::map_error_outcome(other)
            }
        }
    }

    fn map_error_outcome(outcome: EvalOutcome) -> EngineResponse {
        match outcome {
            EvalOutcome::Value { rendered, stdout } => EngineResponse::Output {
                stdout,
                result: Some(rendered),
            },
            EvalOutcome::Declaration { stdout } => EngineResponse::Output {
                stdout,
                result: None,
            },
            EvalOutcome::TypedError { rendered, stdout } => EngineResponse::TypedError {
                stdout,
                message: rendered,
            },
            EvalOutcome::UnhandledEffect { effect, op, stdout } => {
                let short = effect.rsplit('.').next().unwrap_or(&effect).to_owned();
                let hint = format!(
                    "unhandled effect `{short}` (operation `{op}`); \
                     install a handler with `:handle <expr>`"
                );
                EngineResponse::UnhandledEffect {
                    stdout,
                    effect: short,
                    hint,
                }
            }
            EvalOutcome::Interrupted { stdout } => EngineResponse::Interrupted { stdout },
            EvalOutcome::CapabilityDenied {
                capability,
                binding,
                span,
                stdout,
            } => {
                let loc = span.map_or_else(String::new, |s| {
                    format!(" (file {}, bytes {}..{})", s.file, s.start, s.end)
                });
                EngineResponse::Error {
                    stdout,
                    message: format!("capability `{capability}` denied for `{binding}`{loc}"),
                }
            }
            EvalOutcome::LimitExceeded {
                message, stdout, ..
            } => EngineResponse::Error { stdout, message },
            EvalOutcome::Fault { message, stdout } => EngineResponse::Error {
                stdout,
                message: format!("runtime error: {message}"),
            },
        }
    }
}
