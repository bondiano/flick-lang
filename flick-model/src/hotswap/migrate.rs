//! Atomic batch migrations over declared state cells.

use std::collections::BTreeSet;

use super::session::LiveSession;
use super::types::{
    MigratedCell, MigrationBatch, MigrationOutcome, MigrationRollbackReason, render_term,
};
use crate::check::compat::check_migration_fn;
use crate::eval::{Machine, MachineState, Outcome, Program};
use crate::syntax::{Path, Term};
use crate::types::Type;

impl LiveSession {
    /// Attempts an atomic batch migration over declared state cells.
    ///
    /// Every migration function `Old -> New` is type-checked as closed and pure
    /// and evaluated on a read-only view of the current store under `batch.fuel`.
    /// - If every cell migration succeeds with a value, all new values are
    ///   appended to the store (preserving old cells at their original indices
    ///   for suspended frames) and `state_cells` are updated to the new
    ///   locations/types.
    /// - If any cell migration fails type checking, raises, panics, gets stuck,
    ///   or diverges (runs out of fuel), the entire configuration is left
    ///   completely unchanged ([`MigrationOutcome::RolledBack`]).
    #[must_use]
    pub fn migrate(&mut self, batch: &MigrationBatch) -> MigrationOutcome {
        let mut seen = BTreeSet::new();
        let mut staged: Vec<(Path, usize, Type, Type, Term)> =
            Vec::with_capacity(batch.migrations.len());

        for m in &batch.migrations {
            if !seen.insert(m.cell.clone()) {
                return MigrationOutcome::RolledBack {
                    reason: MigrationRollbackReason::DuplicateCellInBatch(m.cell.clone()),
                };
            }
            let Some(entry) = self.state_cells.get(&m.cell) else {
                return MigrationOutcome::RolledBack {
                    reason: MigrationRollbackReason::UnknownStateCell(m.cell.clone()),
                };
            };
            if entry.current_ty != m.old_ty {
                return MigrationOutcome::RolledBack {
                    reason: MigrationRollbackReason::OldTypeMismatch {
                        cell: m.cell.clone(),
                        expected: Box::new(m.old_ty.clone()),
                        actual: Box::new(entry.current_ty.clone()),
                    },
                };
            }
            if let Err(err) = check_migration_fn(&self.decls, &m.old_ty, &m.new_ty, &m.migrate_fn) {
                return MigrationOutcome::RolledBack {
                    reason: MigrationRollbackReason::TypeError(err),
                };
            }
            let Some(old_val) = self.state.mem.get(entry.current_loc).cloned() else {
                return MigrationOutcome::RolledBack {
                    reason: MigrationRollbackReason::RuntimeStuck(format!(
                        "missing store location ℓ{}",
                        entry.current_loc
                    )),
                };
            };
            let call = Term::app(m.migrate_fn.clone(), old_val);
            match eval_pure_term(&self.program, &self.state.mem, call, batch.fuel) {
                Ok(new_val) => {
                    staged.push((
                        m.cell.clone(),
                        entry.current_loc,
                        m.old_ty.clone(),
                        m.new_ty.clone(),
                        new_val,
                    ));
                }
                Err(reason) => return MigrationOutcome::RolledBack { reason },
            }
        }

        // All migrations succeeded: commit atomically by appending new versions.
        let mut migrated = Vec::with_capacity(staged.len());
        for (cell, old_loc, old_ty, new_ty, new_val) in staged {
            let new_loc = self.state.mem.len();
            self.state.mem.push(new_val);
            if let Some(entry) = self.state_cells.get_mut(&cell) {
                entry.current_loc = new_loc;
                entry.current_ty = new_ty.clone();
                entry.history.push((new_loc, new_ty.clone()));
            }
            migrated.push(MigratedCell {
                cell,
                old_loc,
                new_loc,
                old_ty,
                new_ty,
            });
        }
        MigrationOutcome::Committed { migrated }
    }
}

/// Evaluates `term` purely in an isolated scratch machine with read-only store
/// visibility, rejecting any store/region mutation or non-value termination.
pub(super) fn eval_pure_term(
    program: &Program,
    mem_snapshot: &[Term],
    term: Term,
    fuel: u64,
) -> Result<Term, MigrationRollbackReason> {
    let before_mem: Vec<String> = mem_snapshot.iter().map(render_term).collect();
    let scratch = MachineState {
        mem: mem_snapshot.to_vec(),
        fuel,
        ..MachineState::default()
    };
    let mut machine = Machine::from_state(program, scratch);
    let outcome = machine.run(term);
    let after = machine.into_state();
    match outcome {
        Outcome::Value(v) => {
            if !after.lambda.regions.is_empty() {
                return Err(MigrationRollbackReason::ImpureSideEffect(
                    "migration opened a runtime region".into(),
                ));
            }
            let after_mem: Vec<String> = after.mem.iter().map(render_term).collect();
            if after_mem != before_mem {
                return Err(MigrationRollbackReason::ImpureSideEffect(
                    "migration mutated the store".into(),
                ));
            }
            Ok(v)
        }
        Outcome::Raise(v) => Err(MigrationRollbackReason::RuntimeRaise(render_term(&v))),
        Outcome::Die(why) => Err(MigrationRollbackReason::RuntimeDie(why)),
        Outcome::Stuck(why) => Err(MigrationRollbackReason::RuntimeStuck(why)),
        Outcome::OutOfFuel => Err(MigrationRollbackReason::Diverged),
    }
}
