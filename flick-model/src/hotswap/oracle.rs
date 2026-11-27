//! Reference oracle runner for generated hot-swap [`SessionScenario`]s.

use super::scenario::{
    ActionObservation, MigrationKind, SessionAction, SessionOracleError, SessionOracleReport,
    SessionScenario, SwapKind,
};
use super::session::LiveSession;
use super::types::{
    MigrationBatch, MigrationOutcome, SessionOutcomeSummary, StepResult, SwapError, SwapRequest,
    render_term, summarize_outcome,
};
use crate::decls::Decls;
use crate::eval::{Outcome, Program};

fn oracle_err(idx: usize, message: impl Into<String>) -> SessionOracleError {
    SessionOracleError {
        step_index: Some(idx),
        message: message.into(),
    }
}

fn run_swap_action(
    session: &mut LiveSession,
    idx: usize,
    request: &SwapRequest,
    kind: SwapKind,
) -> Result<ActionObservation, SessionOracleError> {
    let before = session.snapshot();
    match session.swap(request.clone()) {
        Ok(()) => {
            if !kind.expected_accepted() {
                return Err(oracle_err(
                    idx,
                    format!(
                        "incompatible swap `{}` ({kind:?}) was unexpectedly accepted",
                        request.target
                    ),
                ));
            }
            session.check_configuration().map_err(|err| {
                oracle_err(
                    idx,
                    format!(
                        "configuration ill-typed after compatible swap `{}`: {err}",
                        request.target
                    ),
                )
            })?;
            Ok(ActionObservation::SwapAccepted {
                target: request.target.clone(),
                kind,
            })
        }
        Err(err) => {
            if kind.expected_accepted() {
                return Err(oracle_err(
                    idx,
                    format!(
                        "compatible swap `{}` ({kind:?}) was unexpectedly rejected: {err}",
                        request.target
                    ),
                ));
            }
            if before != session.snapshot() {
                return Err(oracle_err(
                    idx,
                    format!("rejected swap `{}` mutated session state", request.target),
                ));
            }
            let rule = match &err {
                SwapError::Incompatible(c) => c.rule,
                SwapError::IllTypedReplacement(t) => t.rule,
                SwapError::UnknownGlobal(_) => "Swap",
            };
            Ok(ActionObservation::SwapRejected {
                target: request.target.clone(),
                kind,
                rule,
            })
        }
    }
}

fn run_migrate_action(
    session: &mut LiveSession,
    idx: usize,
    batch: &MigrationBatch,
    kind: MigrationKind,
) -> Result<ActionObservation, SessionOracleError> {
    let before = session.snapshot();
    match session.migrate(batch) {
        MigrationOutcome::Committed { migrated } => {
            if !kind.expected_committed() {
                return Err(oracle_err(
                    idx,
                    format!("migration ({kind:?}) committed when rollback was expected"),
                ));
            }
            for m in &migrated {
                let old_before = before.mem_rendered.get(m.old_loc);
                let old_after = session.mem().get(m.old_loc).map(render_term);
                if old_before != old_after.as_ref() {
                    return Err(oracle_err(
                        idx,
                        format!(
                            "committed migration overwrote old cell ℓ{} of `{}`",
                            m.old_loc, m.cell
                        ),
                    ));
                }
            }
            session.check_configuration().map_err(|err| {
                oracle_err(
                    idx,
                    format!("configuration ill-typed after committed migration: {err}"),
                )
            })?;
            Ok(ActionObservation::MigrationCommitted {
                kind,
                migrated_cells: migrated
                    .into_iter()
                    .map(|m| (m.cell, m.old_loc, m.new_loc))
                    .collect(),
            })
        }
        MigrationOutcome::RolledBack { reason } => {
            if kind.expected_committed() {
                return Err(oracle_err(
                    idx,
                    format!("valid migration ({kind:?}) unexpectedly rolled back: {reason}"),
                ));
            }
            if before != session.snapshot() {
                return Err(oracle_err(
                    idx,
                    format!(
                        "rolled-back migration ({kind:?}) was not atomic: session state changed"
                    ),
                ));
            }
            session.check_configuration().map_err(|err| {
                oracle_err(
                    idx,
                    format!("configuration ill-typed after rolled-back migration: {err}"),
                )
            })?;
            Ok(ActionObservation::MigrationRolledBack {
                kind,
                reason: reason.to_string(),
            })
        }
    }
}

fn run_single_action(
    session: &mut LiveSession,
    idx: usize,
    action: &SessionAction,
    finished_outcome: &mut Option<Outcome>,
) -> Result<ActionObservation, SessionOracleError> {
    match action {
        SessionAction::Step { steps } => {
            let res = session.step_n(*steps, true).map_err(|err| {
                oracle_err(idx, format!("preservation failed during step: {err}"))
            })?;
            match res {
                StepResult::Running { steps_taken } => Ok(ActionObservation::Stepped {
                    steps_taken,
                    finished: false,
                }),
                StepResult::Finished {
                    steps_taken,
                    outcome,
                } => {
                    if let Outcome::Stuck(why) = &outcome {
                        return Err(oracle_err(idx, format!("progress failed: stuck: {why}")));
                    }
                    *finished_outcome = Some(outcome);
                    Ok(ActionObservation::Stepped {
                        steps_taken,
                        finished: true,
                    })
                }
                StepResult::Idle => Ok(ActionObservation::Stepped {
                    steps_taken: 0,
                    finished: true,
                }),
            }
        }
        SessionAction::LoadModule(module) => {
            session.load_module(module.clone()).map_err(|err| {
                oracle_err(
                    idx,
                    format!("module `{}` failed to load: {err}", module.name),
                )
            })?;
            session.check_configuration().map_err(|err| {
                oracle_err(
                    idx,
                    format!("configuration ill-typed after loading module: {err}"),
                )
            })?;
            Ok(ActionObservation::ModuleLoaded {
                name: module.name.clone(),
            })
        }
        SessionAction::Swap { request, kind } => run_swap_action(session, idx, request, *kind),
        SessionAction::Migrate { batch, kind } => run_migrate_action(session, idx, batch, *kind),
        SessionAction::Call { expr, expected_ty } => {
            let outcome = session
                .eval_call(expr.clone(), expected_ty, 10_000)
                .map_err(|err| oracle_err(idx, format!("call failed typing: {err}")))?;
            if let Outcome::Stuck(why) = &outcome {
                return Err(oracle_err(idx, format!("call got stuck: {why}")));
            }
            Ok(ActionObservation::CallResult {
                outcome: summarize_outcome(&outcome),
            })
        }
        SessionAction::ReadState { cell } => {
            let Some(loc) = session.state_cell_loc(cell) else {
                return Err(oracle_err(idx, format!("missing state cell `{cell}`")));
            };
            let outcome = session.read_state(cell, 5_000).map_err(|err| {
                oracle_err(idx, format!("state read `{cell}` failed typing: {err}"))
            })?;
            if let Outcome::Stuck(why) = &outcome {
                return Err(oracle_err(
                    idx,
                    format!("state read `{cell}` got stuck: {why}"),
                ));
            }
            Ok(ActionObservation::StateRead {
                cell: cell.clone(),
                loc,
                outcome: summarize_outcome(&outcome),
            })
        }
    }
}

/// Runs a [`SessionScenario`] on the reference model [`LiveSession`], asserting:
/// - every step preserves configuration typing and never gets stuck;
/// - every compatible swap is accepted and preserves configuration typing;
/// - every incompatible swap is rejected and leaves the full session snapshot
///   unchanged;
/// - every valid migration commits, preserves old state cells at their old
///   types, and preserves configuration typing;
/// - every failing/diverging/partial-batch migration rolls back atomically with
///   the exact pre-migration snapshot unchanged.
///
/// # Errors
/// Returns [`SessionOracleError`] describing the first violated property.
pub fn run_session_oracle(
    base_decls: &Decls,
    base_program: &Program,
    scenario: &SessionScenario,
) -> Result<SessionOracleReport, SessionOracleError> {
    let mut session = LiveSession::new(base_decls.clone(), base_program.clone(), 50_000);
    for module in &scenario.initial_modules {
        session
            .load_module(module.clone())
            .map_err(|err| SessionOracleError {
                step_index: None,
                message: format!("initial module `{}` failed: {err}", module.name),
            })?;
    }
    session
        .start_entry(scenario.entry.clone(), scenario.entry_ty.clone())
        .map_err(|err| SessionOracleError {
            step_index: None,
            message: format!("entry failed typing: {err}"),
        })?;
    session
        .check_configuration()
        .map_err(|err| SessionOracleError {
            step_index: None,
            message: format!("initial configuration failed typing: {err}"),
        })?;

    let mut observations = Vec::with_capacity(scenario.actions.len());
    let mut finished_outcome: Option<Outcome> = None;
    for (idx, action) in scenario.actions.iter().enumerate() {
        observations.push(run_single_action(
            &mut session,
            idx,
            action,
            &mut finished_outcome,
        )?);
    }

    if finished_outcome.is_none() {
        finished_outcome = session.run_to_end(true).map_err(|err| SessionOracleError {
            step_index: None,
            message: format!("preservation failed while finishing entry: {err}"),
        })?;
    }
    let final_outcome = match finished_outcome {
        Some(Outcome::Stuck(why)) => {
            return Err(SessionOracleError {
                step_index: None,
                message: format!("entry ended stuck: {why}"),
            });
        }
        Some(ref outcome) => summarize_outcome(outcome),
        None => SessionOutcomeSummary::Running,
    };
    let state_versions = session
        .state_cells()
        .iter()
        .map(|(p, e)| (p.clone(), e.current_loc, e.history.len()))
        .collect();
    Ok(SessionOracleReport {
        observations,
        final_outcome,
        state_versions,
    })
}
