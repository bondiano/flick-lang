//! Binding-level dependency graph and atomic staging/commit/rollback helpers
//! on [`super::program::SessionProgram`].

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::mem;

use flick_core::ir::memory::{
    self, Block, CodeId, Exit, Global, Instruction, Intrinsic, Name, Operation, Program,
};

use super::hotswap_types::HotSwapError;
use super::program::{SessionProgram, collect_block_codes};

/// Staged replacement functions added to [`SessionProgram`] awaiting atomic commit.
pub(crate) struct StagedProgramSwap {
    pub(crate) added_codes: Vec<CodeId>,
    pub(crate) new_initializers: BTreeMap<Name, CodeId>,
    pub(crate) temp_globals: Vec<Name>,
}

/// Temporary functions/globals loaded solely for running a pure migration.
pub(crate) struct StagedTempProgram {
    pub(crate) added_codes: Vec<CodeId>,
    pub(crate) added_globals: Vec<Name>,
}

impl SessionProgram {
    /// Returns every binding in this session that transitively depends on any
    /// binding in `targets` (via `Operation::Global` references), sorted and
    /// deduplicated by `(module, name)`.
    #[must_use]
    pub(crate) fn reverse_binding_dependents(&self, targets: &[Name]) -> Vec<Name> {
        let mut rev: BTreeMap<Name, BTreeSet<Name>> = BTreeMap::new();
        for global in &self.program().globals {
            let mut visited = BTreeSet::new();
            let mut work = vec![global.initializer];
            let mut refs = BTreeSet::new();
            while let Some(code) = work.pop() {
                if !visited.insert(code) {
                    continue;
                }
                if let Some(func) = self.program().functions.get(&code) {
                    collect_block_global_names(&func.body, &mut refs);
                    let mut callees = BTreeSet::new();
                    collect_block_codes(&func.body, &mut callees);
                    work.extend(callees);
                }
            }
            for dep in refs {
                if dep != global.name {
                    rev.entry(dep).or_default().insert(global.name.clone());
                }
            }
        }

        let target_set: BTreeSet<&Name> = targets.iter().collect();
        let mut seen: BTreeSet<Name> = BTreeSet::new();
        let mut queue: VecDeque<Name> = targets.iter().cloned().collect();
        while let Some(curr) = queue.pop_front() {
            if let Some(callers) = rev.get(&curr) {
                for caller in callers {
                    if !target_set.contains(caller) && seen.insert(caller.clone()) {
                        queue.push_back(caller.clone());
                    }
                }
            }
        }
        seen.into_iter().collect()
    }

    /// Returns whether `name` is currently registered as a global in this session.
    #[must_use]
    pub(crate) fn has_global(&self, name: &Name) -> bool {
        self.program().globals.iter().any(|g| &g.name == name)
    }

    /// Stages replacement functions for `targets` from `incoming` into this
    /// session's program using fresh monotonic `CodeId`s, without updating the
    /// live `globals` table yet.
    ///
    /// Also registers temporary global names (`$swap_stage.<n>`) pointing to
    /// each target's new initializer so replacement bytecode can be verified
    /// and eagerly initialized before commit.
    pub(crate) fn stage_swap(
        &mut self,
        incoming: &Program,
        targets: &[Name],
    ) -> Result<StagedProgramSwap, HotSwapError> {
        memory::validate_program(incoming).map_err(|e| HotSwapError::InvalidIr(e.to_string()))?;
        for target in targets {
            if !self.has_global(target) {
                return Err(HotSwapError::UnknownBinding(target.clone()));
            }
            if !incoming.globals.iter().any(|g| &g.name == target) {
                return Err(HotSwapError::UnknownBinding(target.clone()));
            }
        }

        let mut incoming_clean = incoming.clone();
        // Keep globals list empty during code-only load so live globals are untouched.
        let incoming_globals = mem::take(&mut incoming_clean.globals);
        let explicit_owners = infer_owners_from(&incoming_clean, &incoming_globals);
        let id_map = self.load_with_owners(&incoming_clean, Some(&explicit_owners));
        let added_codes: Vec<CodeId> = id_map.values().copied().collect();

        let mut new_initializers = BTreeMap::new();
        let mut temp_globals = Vec::new();
        for (idx, target) in targets.iter().enumerate() {
            let old_init = incoming_globals
                .iter()
                .find(|g| &g.name == target)
                .map(|g| g.initializer)
                .ok_or_else(|| HotSwapError::UnknownBinding(target.clone()))?;
            let mapped_init = id_map.get(&old_init).copied().unwrap_or(old_init);
            new_initializers.insert(target.clone(), mapped_init);
            let temp_name = Name::new(
                "$hotswap_stage",
                &format!("init_{idx}_{}_{}", target.module, target.name),
            );
            self.push_temp_global(Global {
                name: temp_name.clone(),
                initializer: mapped_init,
            });
            temp_globals.push(temp_name);
        }

        Ok(StagedProgramSwap {
            added_codes,
            new_initializers,
            temp_globals,
        })
    }

    /// Atomically commits a [`StagedProgramSwap`]: updates each target global's
    /// initializer `CodeId`, removes staging entries, and retires replaced code.
    pub(crate) fn commit_staged_swap(&mut self, staged: &StagedProgramSwap) {
        self.remove_temp_globals(&staged.temp_globals);
        let mut retired = Vec::new();
        self.update_global_initializers(&staged.new_initializers, &mut retired);
        self.retire_code_ids(retired);
    }

    /// Rolls back a [`StagedProgramSwap`], removing every added function, span,
    /// owner entry, and temporary global without affecting live state.
    pub(crate) fn rollback_staged_swap(&mut self, staged: &StagedProgramSwap) {
        self.remove_temp_globals(&staged.temp_globals);
        self.remove_added_codes(&staged.added_codes);
    }

    /// Temporarily loads `incoming` (e.g. a pure migration program) under fresh
    /// monotonic `CodeId`s and a unique alias for `entry`, returning a handle
    /// that must be removed with [`Self::cleanup_temp_program`].
    pub(crate) fn stage_temp_program(
        &mut self,
        incoming: &Program,
        entry: &Name,
        tag: &str,
    ) -> Result<(Name, StagedTempProgram), HotSwapError> {
        memory::validate_program(incoming).map_err(|e| HotSwapError::InvalidIr(e.to_string()))?;
        let entry_init = incoming
            .globals
            .iter()
            .find(|g| &g.name == entry)
            .map(|g| g.initializer)
            .ok_or_else(|| HotSwapError::UnknownBinding(entry.clone()))?;

        let mut code_only = incoming.clone();
        let incoming_globals = mem::take(&mut code_only.globals);
        let explicit_owners = infer_owners_from(&code_only, &incoming_globals);
        let id_map = self.load_with_owners(&code_only, Some(&explicit_owners));
        let added_codes: Vec<CodeId> = id_map.values().copied().collect();

        let mapped_entry_init = id_map.get(&entry_init).copied().unwrap_or(entry_init);
        let temp_name = Name::new("$hotswap_migrate", &format!("{tag}_{}", entry.name));
        self.push_temp_global(Global {
            name: temp_name.clone(),
            initializer: mapped_entry_init,
        });
        Ok((
            temp_name.clone(),
            StagedTempProgram {
                added_codes,
                added_globals: vec![temp_name],
            },
        ))
    }

    /// Removes temporary functions and globals added by [`Self::stage_temp_program`].
    pub(crate) fn cleanup_temp_program(&mut self, staged: &StagedTempProgram) {
        self.remove_temp_globals(&staged.added_globals);
        self.remove_added_codes(&staged.added_codes);
    }

    fn push_temp_global(&mut self, global: Global) {
        self.known_globals.insert(global.name.clone());
        self.program.globals.push(global);
    }

    fn remove_temp_globals(&mut self, names: &[Name]) {
        let remove_set: BTreeSet<&Name> = names.iter().collect();
        self.program
            .globals
            .retain(|g| !remove_set.contains(&g.name));
        self.known_globals.retain(|n| !remove_set.contains(n));
    }

    fn remove_added_codes(&mut self, codes: &[CodeId]) {
        let remove_set: BTreeSet<CodeId> = codes.iter().copied().collect();
        for id in codes {
            self.program.functions.remove(id);
            self.code_owner.remove(id);
        }
        self.program
            .op_spans
            .retain(|&(code, _), _| !remove_set.contains(&code));
    }

    fn update_global_initializers(
        &mut self,
        new_initializers: &BTreeMap<Name, CodeId>,
        retired: &mut Vec<(String, CodeId)>,
    ) {
        for global in &mut self.program.globals {
            if let Some(&new_init) = new_initializers.get(&global.name) {
                let old_init = mem::replace(&mut global.initializer, new_init);
                if old_init != new_init {
                    retired.push((global.name.module.clone(), old_init));
                }
            }
        }
    }

    fn retire_code_ids(&mut self, retired: Vec<(String, CodeId)>) {
        for (module, root_code) in retired {
            let mut visited = BTreeSet::new();
            let mut work = vec![root_code];
            while let Some(code) = work.pop() {
                if !visited.insert(code) {
                    continue;
                }
                if let Some(func) = self.program.functions.get(&code) {
                    let mut refs = BTreeSet::new();
                    collect_block_codes(&func.body, &mut refs);
                    work.extend(refs);
                }
            }
            self.retired_codes
                .entry(module)
                .or_default()
                .extend(visited);
        }
    }
}

/// Verifies that a migration `Program` contains no disallowed side effects
/// (effect `Perform`, `ForeignCall`, `RefSet`/`RefModify`, or host service ops).
pub(crate) fn check_migration_purity(program: &Program, entry: &Name) -> Result<(), String> {
    let Some(init) = program
        .globals
        .iter()
        .find(|g| &g.name == entry)
        .map(|g| g.initializer)
    else {
        return Err(format!(
            "missing migration entry `{}.{}`",
            entry.module, entry.name
        ));
    };
    let mut visited = BTreeSet::new();
    let mut work = vec![init];
    while let Some(code) = work.pop() {
        if !visited.insert(code) {
            continue;
        }
        let Some(func) = program.functions.get(&code) else {
            continue;
        };
        check_block_purity(&func.body)?;
        let mut refs = BTreeSet::new();
        collect_block_codes(&func.body, &mut refs);
        work.extend(refs);
    }
    Ok(())
}

fn check_block_purity(block: &Block) -> Result<(), String> {
    for inst in &block.instructions {
        let Instruction::Let { value, .. } = inst else {
            continue;
        };
        match value {
            Operation::Perform { effect, op, .. } => {
                return Err(format!("migration cannot perform effect `{effect}.{op}`"));
            }
            Operation::ForeignCall { spec, .. } => {
                return Err(format!(
                    "migration cannot call foreign binding `{}`",
                    spec.name
                ));
            }
            Operation::Intrinsic { kind, .. } => match kind {
                Intrinsic::RefSet | Intrinsic::RefModify | Intrinsic::RefMake => {
                    return Err("migration cannot mutate Ref cells".to_owned());
                }
                Intrinsic::Host(_) | Intrinsic::HostService(_) | Intrinsic::PrintLine => {
                    return Err(format!(
                        "migration cannot invoke host/IO intrinsic `{kind:?}`"
                    ));
                }
                _ => {}
            },
            _ => {}
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        check_block_purity(then_)?;
        check_block_purity(else_)?;
    }
    Ok(())
}

fn collect_block_global_names(block: &Block, out: &mut BTreeSet<Name>) {
    for inst in &block.instructions {
        if let Instruction::Let {
            value: Operation::Global(name),
            ..
        } = inst
        {
            out.insert(name.clone());
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        collect_block_global_names(then_, out);
        collect_block_global_names(else_, out);
    }
}

fn infer_owners_from(program: &Program, globals: &[Global]) -> BTreeMap<CodeId, String> {
    let mut owners = BTreeMap::new();
    for global in globals {
        let mut work = vec![global.initializer];
        while let Some(id) = work.pop() {
            if owners.contains_key(&id) {
                continue;
            }
            owners.insert(id, global.name.module.clone());
            if let Some(func) = program.functions.get(&id) {
                let mut refs = BTreeSet::new();
                collect_block_codes(&func.body, &mut refs);
                work.extend(refs);
            }
        }
    }
    owners
}
