//! Declared state-cell registry and atomic pure `Old -> New` migration staging
//! on [`super::VmSession`].

use std::slice;

use flick_core::iface::Interface;
use flick_core::ir::memory::Name;
use flick_core::types::hot_swap::{check_migration_scheme, types_mutually_compatible};
use flick_core::types::{Type, show_type};

use crate::sandbox::{CapabilitySet, SandboxConfig, SandboxError, check_program_capabilities};
use crate::tier0::{
    Data, Heap, ObjectId, ProgramOutcome, SessionRunSpec, Value, execute_session_apply_unary,
    execute_session_step,
};

use super::format::render_value;
use super::hotswap_types::{
    HotSwapError, MigratedCellReport, MigrationBatchRequest, StateCellSpec, StateMigration,
};
use super::program::SessionProgram;
use super::program_swap::check_migration_purity;
use super::{EvalOutcome, VmSession};

/// Metadata tracked for each declared live state cell on [`VmSession`].
#[derive(Debug, Clone)]
pub(crate) struct StateCellRecord {
    pub(crate) ty: Type,
    pub(crate) is_ref: bool,
    pub(crate) version: usize,
}

pub(super) struct StagedMigrationValue {
    pub(super) cell: Name,
    pub(super) new_ty: Type,
    pub(super) is_ref: bool,
    pub(super) new_value: Value,
    pub(super) old_cell_id: Option<ObjectId>,
}

impl VmSession {
    /// Declares a loaded binding as a live state cell eligible for atomic
    /// typed `Old -> New` migrations.
    ///
    /// # Errors
    /// Returns [`HotSwapError::UnknownBinding`] if `spec.name` is not loaded.
    pub fn declare_state_cell(&mut self, spec: StateCellSpec) -> Result<(), HotSwapError> {
        if !self.program.has_global(&spec.name) {
            return Err(HotSwapError::UnknownBinding(spec.name));
        }
        self.state_cells.insert(
            spec.name,
            StateCellRecord {
                ty: spec.ty,
                is_ref: spec.is_ref,
                version: 0,
            },
        );
        Ok(())
    }

    /// Returns the current declared type of a state cell, if declared.
    #[must_use]
    pub fn state_cell_type(&self, name: &Name) -> Option<&Type> {
        self.state_cells.get(name).map(|c| &c.ty)
    }

    /// Returns the migration version counter of a declared state cell.
    #[must_use]
    pub fn state_cell_version(&self, name: &Name) -> Option<usize> {
        self.state_cells.get(name).map(|c| c.version)
    }

    /// Reads and renders the current value of a declared state cell.
    ///
    /// # Errors
    /// Returns [`HotSwapError::UnknownStateCell`] if `name` is not declared.
    pub fn read_state_cell(&mut self, name: &Name) -> Result<EvalOutcome, HotSwapError> {
        let Some(rec) = self.state_cells.get(name).cloned() else {
            return Err(HotSwapError::UnknownStateCell(name.clone()));
        };
        let (val, _) = self.read_live_state_value(name, rec.is_ref)?;
        let rendered = render_value(&self.heap, &val);
        let drop_res = self.heap.drop_value(val);
        match (rendered, drop_res) {
            (Ok(rendered), Ok(())) => Ok(EvalOutcome::Value {
                rendered,
                stdout: String::new(),
            }),
            (Err(err), _) | (_, Err(err)) => Err(HotSwapError::Vm(err.to_string())),
        }
    }

    pub(super) fn validate_migration_batch(
        &self,
        batch: &MigrationBatchRequest,
        interfaces: &[Interface],
        built: &[Interface],
    ) -> Result<(), HotSwapError> {
        for mig in &batch.migrations {
            let Some(rec) = self.state_cells.get(&mig.cell) else {
                return Err(HotSwapError::UnknownStateCell(mig.cell.clone()));
            };
            if !types_mutually_compatible(&rec.ty, &mig.old_ty, interfaces, built) {
                return Err(HotSwapError::StateTypeMismatch {
                    cell: mig.cell.clone(),
                    expected_old: show_type(&mig.old_ty),
                    actual: show_type(&rec.ty),
                });
            }
            if let Some(scheme) = &mig.migrate_scheme
                && let Err(compatibility) =
                    check_migration_scheme(scheme, &mig.old_ty, &mig.new_ty, interfaces, built)
            {
                return Err(HotSwapError::MigrationTypeError {
                    cell: mig.cell.clone(),
                    compatibility: Box::new(compatibility),
                });
            }
            if let Err(err) = check_program_capabilities(
                &mig.migrate_program,
                Some(slice::from_ref(&mig.migrate_entry)),
                CapabilitySet::pure(),
            ) {
                return Err(match err {
                    SandboxError::CapabilityDenied {
                        capability,
                        binding,
                        span,
                    } => HotSwapError::CapabilityDenied {
                        capability,
                        binding,
                        span,
                    },
                    other => HotSwapError::InvalidIr(other.to_string()),
                });
            }
            if let Err(reason) = check_migration_purity(&mig.migrate_program, &mig.migrate_entry) {
                return Err(HotSwapError::MigrationFailed {
                    cell: mig.cell.clone(),
                    reason,
                });
            }
        }
        Ok(())
    }

    pub(super) fn run_staged_migrations(
        &mut self,
        batch: &MigrationBatchRequest,
    ) -> Result<Vec<StagedMigrationValue>, HotSwapError> {
        let pure_sandbox = SandboxConfig {
            capabilities: CapabilitySet::pure(),
            limits: batch.limits.clone(),
        };
        let mut staged: Vec<StagedMigrationValue> = Vec::with_capacity(batch.migrations.len());
        for (idx, mig) in batch.migrations.iter().enumerate() {
            let rec = self
                .state_cells
                .get(&mig.cell)
                .cloned()
                .ok_or_else(|| HotSwapError::UnknownStateCell(mig.cell.clone()))?;
            let (current_val, old_cell_id) = match self.read_live_state_value(&mig.cell, rec.is_ref)
            {
                Ok(pair) => pair,
                Err(err) => {
                    self.drop_staged_migrations(staged);
                    return Err(err);
                }
            };
            match self.run_one_migration(idx, mig, current_val, &pure_sandbox) {
                Ok(new_value) => staged.push(StagedMigrationValue {
                    cell: mig.cell.clone(),
                    new_ty: mig.new_ty.clone(),
                    is_ref: rec.is_ref,
                    new_value,
                    old_cell_id,
                }),
                Err(err) => {
                    self.drop_staged_migrations(staged);
                    return Err(err);
                }
            }
        }
        Ok(staged)
    }

    pub(super) fn commit_staged_migrations(
        &mut self,
        staged_migrations: Vec<StagedMigrationValue>,
    ) -> Result<(Vec<MigratedCellReport>, Vec<Name>), HotSwapError> {
        let mut reports = Vec::with_capacity(staged_migrations.len());
        let mut mutated = Vec::with_capacity(staged_migrations.len());
        for staged_mig in staged_migrations {
            let new_cell_id = if staged_mig.is_ref {
                // Two-version store typing: allocate a new Cell for the migrated
                // value while keeping the pre-migration Cell intact for frames
                // and closures that already captured the old CellHandle.
                let cell_obj = self.heap.allocate(Data::Cell {
                    execution: self.execution,
                    contents: Some(staged_mig.new_value),
                });
                let id = cell_obj.object_id().expect("allocated cell object id");
                self.persistent.cells.push(cell_obj);
                let handle = Heap::cell_handle(id, self.execution);
                if let Some(old_global) = self
                    .persistent
                    .globals
                    .insert(staged_mig.cell.clone(), handle)
                {
                    self.heap
                        .drop_value(old_global)
                        .map_err(|e| HotSwapError::Vm(e.to_string()))?;
                }
                Some(id)
            } else {
                if let Some(old_global) = self
                    .persistent
                    .globals
                    .insert(staged_mig.cell.clone(), staged_mig.new_value)
                {
                    self.heap
                        .drop_value(old_global)
                        .map_err(|e| HotSwapError::Vm(e.to_string()))?;
                }
                None
            };
            let version = if let Some(rec) = self.state_cells.get_mut(&staged_mig.cell) {
                rec.ty = staged_mig.new_ty;
                rec.version += 1;
                rec.version
            } else {
                1
            };
            mutated.push(staged_mig.cell.clone());
            reports.push(MigratedCellReport {
                cell: staged_mig.cell,
                version,
                old_cell_id: staged_mig.old_cell_id,
                new_cell_id,
            });
        }
        Ok((reports, mutated))
    }

    pub(super) fn drop_staged_migrations(&mut self, staged: Vec<StagedMigrationValue>) {
        let vals: Vec<Value> = staged.into_iter().map(|s| s.new_value).collect();
        let _ = self.heap.drop_values(vals);
    }

    fn run_one_migration(
        &mut self,
        idx: usize,
        mig: &StateMigration,
        current_val: Value,
        sandbox: &SandboxConfig,
    ) -> Result<Value, HotSwapError> {
        let (temp_entry, temp_prog) = match self.program.stage_temp_program(
            &mig.migrate_program,
            &mig.migrate_entry,
            &format!("m{idx}"),
        ) {
            Ok(pair) => pair,
            Err(err) => {
                let _ = self.heap.drop_value(current_val);
                return Err(err);
            }
        };
        let report = execute_session_apply_unary(
            &mut self.heap,
            self.program.program(),
            self.execution,
            &mut self.persistent,
            &temp_entry,
            current_val,
            self.host_mode,
            sandbox,
        );
        if let Some(cached) = self.persistent.globals.remove(&temp_entry) {
            let _ = self.heap.drop_value(cached);
        }
        self.program.cleanup_temp_program(&temp_prog);
        match report.outcome {
            ProgramOutcome::Returned(val) => Ok(val),
            other => Err(HotSwapError::MigrationFailed {
                cell: mig.cell.clone(),
                reason: self.describe_failed_outcome(other),
            }),
        }
    }

    fn read_live_state_value(
        &mut self,
        name: &Name,
        is_ref: bool,
    ) -> Result<(Value, Option<ObjectId>), HotSwapError> {
        if !self.persistent.globals.contains_key(name) {
            let spec = SessionRunSpec {
                entry: name,
                await_task: false,
                root_layer: self.root_layer.as_ref(),
                provided_layers: &[],
                handlers: &[],
                fuel: self.fuel,
                host_mode: self.host_mode,
                program_args: &self.program_args,
                external_interrupt: None,
                synthetic_codes: SessionProgram::synthetic_codes(),
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
                ProgramOutcome::Returned(v) => {
                    self.heap
                        .drop_value(v)
                        .map_err(|e| HotSwapError::Vm(e.to_string()))?;
                }
                other => {
                    let reason = self.describe_failed_outcome(other);
                    return Err(HotSwapError::MigrationFailed {
                        cell: name.clone(),
                        reason,
                    });
                }
            }
        }
        let global_val = self
            .persistent
            .globals
            .get(name)
            .ok_or_else(|| HotSwapError::UnknownStateCell(name.clone()))?;
        if is_ref {
            let Value::Cell(handle) = global_val else {
                return Err(HotSwapError::MigrationFailed {
                    cell: name.clone(),
                    reason: "declared Ref state cell does not hold a Cell handle".to_owned(),
                });
            };
            let cell_id = handle.id;
            let contents_rep = match self.heap.object(cell_id) {
                Ok(obj) => match &obj.data {
                    Data::Cell {
                        contents: Some(val),
                        ..
                    } => val.representation(),
                    _ => {
                        return Err(HotSwapError::MigrationFailed {
                            cell: name.clone(),
                            reason: "declared Ref state cell is empty or closed".to_owned(),
                        });
                    }
                },
                Err(err) => return Err(HotSwapError::Vm(err.to_string())),
            };
            let dup = self
                .heap
                .dup(&contents_rep)
                .map_err(|e| HotSwapError::Vm(e.to_string()))?;
            Ok((dup, Some(cell_id)))
        } else {
            let rep = global_val.representation();
            let dup = self
                .heap
                .dup(&rep)
                .map_err(|e| HotSwapError::Vm(e.to_string()))?;
            Ok((dup, None))
        }
    }
}
