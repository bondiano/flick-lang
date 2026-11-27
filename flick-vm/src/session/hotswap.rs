//! Live hot swap, bytecode verification, and cache/region invalidation on
//! [`super::VmSession`].

use std::collections::BTreeSet;
use std::slice;

use flick_core::iface::{Interface, Namespace};
use flick_core::ir::memory::{CodeId, Name, Program};
use flick_core::types::hot_swap::check_scheme_subsumes;
use flick_core::types::{Type, builtins};

use crate::sandbox::{SandboxError, check_program_capabilities, verify_bytecode};
use crate::tier0::{ProgramOutcome, SessionRunSpec, Value, execute_session_step};
use crate::tier1::opcode::{PrepareError, Prepared};
use crate::tier1::session as tier1_session;

use super::hotswap_types::{
    ExecutionTier, HotSwapError, MigrationBatchRequest, RegionInvalidator, SwapDefinitionsRequest,
    SwapReport,
};
use super::program::SessionProgram;
use super::program_swap::StagedProgramSwap;
use super::stepper::SuspendedTask;
use super::{EvalOutcome, VmSession};

impl VmSession {
    /// Returns the default execution tier for this session.
    #[must_use]
    pub const fn tier(&self) -> ExecutionTier {
        self.tier
    }

    /// Sets the default execution tier for this session.
    pub fn set_tier(&mut self, tier: ExecutionTier) {
        self.tier = tier;
    }

    /// Registers a [`RegionInvalidator`] hook notified on every committed hot swap.
    pub fn register_region_invalidator(&mut self, invalidator: Box<dyn RegionInvalidator>) {
        self.region_invalidators.push(invalidator);
    }

    /// Registers or updates the declared Flick type scheme of a top-level binding.
    pub fn register_binding_scheme(&mut self, name: Name, scheme: Type) {
        self.binding_schemes.insert(name, scheme);
    }

    /// Returns the known Flick type scheme for `name` (either explicitly
    /// registered or exported by its loaded module [`Interface`]).
    #[must_use]
    pub fn binding_scheme(&self, name: &Name) -> Option<&Type> {
        if let Some(scheme) = self.binding_schemes.get(name) {
            return Some(scheme);
        }
        self.modules
            .get(&name.module)
            .and_then(|iface| iface.find(Namespace::Value, &name.name))
            .and_then(|export| export.scheme.as_ref())
    }

    /// Atomically hot-swaps top-level definitions at a safe point.
    ///
    /// Enforces exact scheme subsumption (`σ_new ≤ σ_old`), verifies
    /// replacement bytecode through `#992`'s bytecode verifier, invalidates
    /// cached tier-1 bytecode and registered [`RegionInvalidator`] hooks, and
    /// preserves old code for active frames and pre-swap closures.
    ///
    /// # Errors
    /// Returns [`HotSwapError`] and leaves all code, globals, and heap slots
    /// unchanged when any compatibility, capability, verification, or
    /// initialization check fails.
    pub fn swap_definitions(
        &mut self,
        request: SwapDefinitionsRequest,
    ) -> Result<SwapReport, HotSwapError> {
        self.swap_with_migration(Some(request), None)
    }

    /// Atomically migrates one or more declared state cells using pure
    /// `Old -> New` functions under sandboxed resource limits.
    ///
    /// # Errors
    /// Returns [`HotSwapError`] and rolls back every cell cleanly with zero
    /// heap leaks if any migration fails type checking, raises, defects,
    /// performs disallowed side effects, or exhausts fuel/limits.
    pub fn migrate_state_cells(
        &mut self,
        batch: &MigrationBatchRequest,
    ) -> Result<SwapReport, HotSwapError> {
        self.swap_with_migration(None, Some(batch))
    }

    /// Atomically performs a definition hot swap and/or a typed state-cell
    /// migration batch as one all-or-nothing transaction.
    ///
    /// # Errors
    /// Returns [`HotSwapError`] with zero mutations to live code, globals,
    /// state cells, or heap allocations if any check or migration fails.
    #[allow(clippy::too_many_lines)]
    pub fn swap_with_migration(
        &mut self,
        swap: Option<SwapDefinitionsRequest>,
        migration: Option<&MigrationBatchRequest>,
    ) -> Result<SwapReport, HotSwapError> {
        let interfaces = self.all_interfaces();
        let built = builtins();

        // Phase 1: Validate swap targets, capabilities, and exact scheme subsumption.
        let target_names: Vec<Name> = swap
            .as_ref()
            .map(|req| req.targets.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default();
        if let Some(req) = &swap {
            self.validate_swap_request(req, &target_names, &interfaces, built)?;
        }

        // Phase 2: Validate migration batch types, declared state cells, capabilities, and purity.
        if let Some(batch) = migration {
            self.validate_migration_batch(batch, &interfaces, built)?;
        }

        // Phase 3: Stage replacement IR and verify bytecode (#992).
        let staged_swap = match &swap {
            Some(req) => Some(self.program.stage_swap(&req.incoming, &target_names)?),
            None => None,
        };
        let mut bytecode_verified = false;
        if let Some(staged) = &staged_swap {
            match self.verify_staged_swap_bytecode(staged) {
                Ok(verified) => bytecode_verified = verified,
                Err(err) => {
                    if let Some(staged) = &staged_swap {
                        self.program.rollback_staged_swap(staged);
                    }
                    return Err(err);
                }
            }
        }

        // Phase 4: Execute staged pure migrations (rolling back on any failure).
        let staged_migrations = if let Some(batch) = migration {
            match self.run_staged_migrations(batch) {
                Ok(values) => values,
                Err(err) => {
                    if let Some(staged) = &staged_swap {
                        self.program.rollback_staged_swap(staged);
                    }
                    return Err(err);
                }
            }
        } else {
            Vec::new()
        };

        // Phase 5: Eagerly initialize swapped globals into staged values.
        let staged_swap_values = if let (Some(req), Some(staged)) = (&swap, &staged_swap)
            && req.eager_initialize
        {
            match self.initialize_staged_swaps(staged, &target_names) {
                Ok(vals) => vals,
                Err(err) => {
                    self.drop_staged_migrations(staged_migrations);
                    if let Some(staged) = &staged_swap {
                        self.program.rollback_staged_swap(staged);
                    }
                    return Err(err);
                }
            }
        } else {
            Vec::new()
        };

        // Phase 6: Stage extended Tier-1 Prepared images for paused Tier-1 tasks.
        let staged_tier1_extensions = if let Some(staged) = &staged_swap {
            match self.stage_tier1_task_extensions(staged, &target_names) {
                Ok(exts) => exts,
                Err(err) => {
                    self.drop_staged_swap_values(staged_swap_values);
                    self.drop_staged_migrations(staged_migrations);
                    if let Some(staged) = &staged_swap {
                        self.program.rollback_staged_swap(staged);
                    }
                    return Err(err);
                }
            }
        } else {
            Vec::new()
        };

        // Phase 7: ATOMIC COMMIT.
        if let Some(staged) = &staged_swap {
            self.program.commit_staged_swap(staged);
        }
        if let Some(req) = swap {
            for iface in req.updated_interfaces {
                self.modules.insert(iface.module.clone(), iface);
            }
            for target in req.targets {
                if let Some(scheme) = target.new_scheme {
                    self.binding_schemes.insert(target.name, scheme);
                }
            }
            if req.eager_initialize {
                for (name, new_val) in staged_swap_values {
                    if let Some(old_val) = self.persistent.globals.insert(name, new_val) {
                        self.heap
                            .drop_value(old_val)
                            .map_err(|e| HotSwapError::Vm(e.to_string()))?;
                    }
                }
            } else {
                for name in &target_names {
                    if let Some(old_val) = self.persistent.globals.remove(name) {
                        self.heap
                            .drop_value(old_val)
                            .map_err(|e| HotSwapError::Vm(e.to_string()))?;
                    }
                }
            }
        }

        let (migrated_cells, mutated_cells) = self.commit_staged_migrations(staged_migrations)?;

        for (task_id, extended) in staged_tier1_extensions {
            if let Some(SuspendedTask::Tier1(t1)) = self.step_tasks.get_mut(&task_id) {
                t1.commit_hot_swap(extended);
            }
        }

        let mut root_changed = target_names.clone();
        for c in &mutated_cells {
            if !root_changed.contains(c) {
                root_changed.push(c.clone());
            }
        }
        let invalidated_bindings = if root_changed.is_empty() {
            Vec::new()
        } else {
            let mut affected: BTreeSet<Name> = root_changed.iter().cloned().collect();
            affected.extend(self.program.reverse_binding_dependents(&root_changed));
            let affected_vec: Vec<Name> = affected.into_iter().collect();
            self.invalidate_prepared_caches(&affected_vec);
            affected_vec
        };

        let pinned = self.all_pinned_codes();
        self.program.reclaim_retired_codes(&pinned);

        Ok(SwapReport {
            swapped_bindings: target_names,
            invalidated_bindings,
            migrated_cells,
            bytecode_verified,
        })
    }

    pub(crate) fn all_pinned_codes(&self) -> BTreeSet<CodeId> {
        let mut pinned = self.heap.live_code_ids();
        if let Some(active) = self.active_eval.as_deref() {
            pinned.extend(active.pinned_codes());
        }
        for task in self.step_tasks.values() {
            pinned.extend(task.pinned_codes());
        }
        pinned
    }

    pub(super) fn describe_failed_outcome(&mut self, outcome: ProgramOutcome) -> String {
        match self.classify_non_value_outcome(outcome, String::new()) {
            EvalOutcome::TypedError { rendered, .. } => format!("raised error `{rendered}`"),
            EvalOutcome::UnhandledEffect { effect, op, .. } => {
                format!("unhandled effect `{effect}.{op}`")
            }
            EvalOutcome::Interrupted { .. } => "interrupted".to_owned(),
            EvalOutcome::CapabilityDenied {
                capability,
                binding,
                ..
            } => format!("capability `{capability}` denied for `{binding}`"),
            EvalOutcome::LimitExceeded { message, .. } | EvalOutcome::Fault { message, .. } => {
                message
            }
            EvalOutcome::Value { .. } | EvalOutcome::Declaration { .. } => {
                "unexpected outcome".to_owned()
            }
        }
    }

    fn all_interfaces(&self) -> Vec<Interface> {
        self.modules.values().cloned().collect()
    }

    fn invalidate_prepared_caches(&mut self, affected: &[Name]) {
        if affected.is_empty() {
            return;
        }
        let affected_set: BTreeSet<&Name> = affected.iter().collect();
        self.tier1_cache.retain(|(entry, _), cached| {
            !affected_set.contains(entry) && !cached.deps.iter().any(|d| affected_set.contains(d))
        });
        for invalidator in &mut self.region_invalidators {
            invalidator.invalidate(affected);
        }
    }

    fn validate_swap_request(
        &self,
        req: &SwapDefinitionsRequest,
        target_names: &[Name],
        interfaces: &[Interface],
        built: &[Interface],
    ) -> Result<(), HotSwapError> {
        if let Err(err) =
            check_program_capabilities(&req.incoming, Some(target_names), self.sandbox.capabilities)
        {
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
        for target in &req.targets {
            if !self.program.has_global(&target.name) {
                return Err(HotSwapError::UnknownBinding(target.name.clone()));
            }
            if !req.incoming.globals.iter().any(|g| g.name == target.name) {
                return Err(HotSwapError::UnknownBinding(target.name.clone()));
            }
            if let Some(new_scheme) = &target.new_scheme
                && let Some(old_scheme) = self.binding_scheme(&target.name)
                && let Err(compatibility) =
                    check_scheme_subsumes(new_scheme, old_scheme, interfaces, built)
            {
                let dependents = self
                    .program
                    .reverse_binding_dependents(slice::from_ref(&target.name));
                return Err(HotSwapError::Incompatible {
                    target: target.name.clone(),
                    compatibility: Box::new(compatibility),
                    dependents,
                });
            }
        }
        Ok(())
    }

    fn verify_staged_swap_bytecode(
        &self,
        staged: &StagedProgramSwap,
    ) -> Result<bool, HotSwapError> {
        let extra_codes = self.all_pinned_codes();
        let mut all_verified = true;
        for temp_name in &staged.temp_globals {
            match tier1_session::prepare_session(
                self.program.program(),
                temp_name,
                None,
                &extra_codes,
            ) {
                Ok(prepared) => {
                    verify_bytecode(&prepared).map_err(HotSwapError::BytecodeVerification)?;
                }
                Err(PrepareError::Unsupported { .. }) if self.tier == ExecutionTier::Tier0 => {
                    all_verified = false;
                }
                Err(err) => {
                    return Err(HotSwapError::InvalidIr(err.to_string()));
                }
            }
        }
        Ok(all_verified)
    }

    fn stage_tier1_task_extensions(
        &self,
        staged: &StagedProgramSwap,
        target_names: &[Name],
    ) -> Result<Vec<(super::StepTaskId, Prepared)>, HotSwapError> {
        let mut candidate_program = self.program.program().clone();
        for global in &mut candidate_program.globals {
            if let Some(&new_init) = staged.new_initializers.get(&global.name) {
                global.initializer = new_init;
            }
        }
        let extra_codes = self.all_pinned_codes();
        let mut extensions = Vec::new();
        for (&task_id, task) in &self.step_tasks {
            if let SuspendedTask::Tier1(t1) = task {
                let ext = t1
                    .stage_hot_swap(&candidate_program, target_names, &extra_codes)
                    .map_err(|e| HotSwapError::InvalidIr(e.to_string()))?;
                extensions.push((task_id, ext));
            }
        }
        Ok(extensions)
    }

    fn initialize_staged_swaps(
        &mut self,
        staged: &StagedProgramSwap,
        targets: &[Name],
    ) -> Result<Vec<(Name, Value)>, HotSwapError> {
        let mut results: Vec<(Name, Value)> = Vec::with_capacity(targets.len());
        for (target, temp_name) in targets.iter().zip(&staged.temp_globals) {
            let spec = SessionRunSpec {
                entry: temp_name,
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
            if let Some(cached) = self.persistent.globals.remove(temp_name) {
                let _ = self.heap.drop_value(cached);
            }
            match report.outcome {
                ProgramOutcome::Returned(val) => results.push((target.clone(), val)),
                other => {
                    let reason = self.describe_failed_outcome(other);
                    self.drop_staged_swap_values(results);
                    return Err(HotSwapError::InitializationFailed {
                        target: target.clone(),
                        reason,
                    });
                }
            }
        }
        Ok(results)
    }

    fn drop_staged_swap_values(&mut self, staged: Vec<(Name, Value)>) {
        let vals: Vec<Value> = staged.into_iter().map(|(_, v)| v).collect();
        let _ = self.heap.drop_values(vals);
    }
}

/// Prepares and structurally verifies a tier-1 [`Prepared`] image (`#992`).
pub(super) fn prepare_verified_tier1(
    program: &Program,
    entry: &Name,
    root: Option<&Name>,
    extra_codes: &BTreeSet<CodeId>,
) -> Result<Prepared, HotSwapError> {
    let prepared = tier1_session::prepare_session(program, entry, root, extra_codes)
        .map_err(|e| HotSwapError::InvalidIr(e.to_string()))?;
    verify_bytecode(&prepared).map_err(HotSwapError::BytecodeVerification)?;
    Ok(prepared)
}
