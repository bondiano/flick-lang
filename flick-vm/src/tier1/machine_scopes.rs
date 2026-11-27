//! Full C0–C6 scope ownership, child cancellation, hooks, LIFO finalizers,
//! and exact-once arena retirement for tier 1.

use std::collections::{BTreeSet, VecDeque};
use std::mem;

use crate::tier0::{CauseToken, Data, DefectSpec, Error, ObjectId, OwnerHandle, Value};

use super::machine::{End, Machine, OwnerEvidence, RegVal};
use super::machine_unwind::Discard;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnerPhase {
    Open,
    Closing,
    Finalizing,
    Closed,
}

pub(super) enum OwnerHook {
    StopFiberGroup(usize),
}

pub(super) enum Finalizer {
    Task {
        task: RegVal,
        services: Vec<Value>,
    },
    WithCause {
        func: RegVal,
        services: Vec<Value>,
    },
    WithExit {
        func: RegVal,
        services: Vec<Value>,
    },
    Release {
        value: Value,
        release_fn: RegVal,
        services: Vec<Value>,
    },
    CloseChildOwner(usize),
    ReleaseMemo {
        memo_id: usize,
        layer_id: ObjectId,
        entry_owner: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ClosePhase {
    Hooks,
    Children,
    PostHooks,
    Finalizers,
}

pub(super) struct OwnerState {
    pub(super) phase: OwnerPhase,
    pub(super) exit_cause: Option<CauseToken>,
    pub(super) hooks: VecDeque<OwnerHook>,
    pub(super) children: BTreeSet<usize>,
    pub(super) watchers: usize,
    pub(super) close_waiter: Option<usize>,
    pub(super) finalizers: Vec<Finalizer>,
    pub(super) arenas: Vec<Value>,
}

impl Default for OwnerState {
    fn default() -> Self {
        Self {
            phase: OwnerPhase::Open,
            exit_cause: None,
            hooks: VecDeque::new(),
            children: BTreeSet::new(),
            watchers: 0,
            close_waiter: None,
            finalizers: Vec::new(),
            arenas: Vec::new(),
        }
    }
}

impl OwnerState {
    pub(super) fn is_open(&self) -> bool {
        self.phase == OwnerPhase::Open
    }

    pub(super) fn is_closing_or_closed(&self) -> bool {
        self.phase != OwnerPhase::Open
    }

    pub(super) fn is_finalizing(&self) -> bool {
        self.phase == OwnerPhase::Finalizing
    }

    pub(super) fn is_finalizing_or_closed(&self) -> bool {
        matches!(self.phase, OwnerPhase::Finalizing | OwnerPhase::Closed)
    }

    pub(super) fn is_closed(&self) -> bool {
        self.phase == OwnerPhase::Closed
    }
}

pub(super) enum BracketOutcome {
    Success(RegVal),
    Failure(CauseToken),
}

pub(super) enum ScopeOutcome {
    Normal {
        dst: Option<usize>,
        value: RegVal,
    },
    Failure(CauseToken),
    Discard(Box<Discard>),
    ChildClose {
        parent: Box<ClosingScopeState>,
        inherited_exit: Option<CauseToken>,
    },
}

pub(super) struct ClosingScopeState {
    pub(super) owner: usize,
    pub(super) previous: usize,
    pub(super) phase: ClosePhase,
    pub(super) remaining: Vec<Finalizer>,
    pub(super) outcome: ScopeOutcome,
    pub(super) exit_override: Option<CauseToken>,
    pub(super) finalizer_failure: Option<CauseToken>,
    pub(super) elect_on_fail: bool,
    pub(super) saved_mask: usize,
    pub(super) masked: bool,
}

impl Machine<'_, '_> {
    pub(super) fn owner_handle(&self, index: usize) -> OwnerHandle {
        OwnerHandle {
            heap: self.heap.identity,
            execution: self.execution,
            index,
        }
    }

    pub(super) fn owner_index(&self, value: &Value) -> Result<usize, Error> {
        let Value::Owner(owner) = value else {
            return Err(Error::ExpectedOwner);
        };
        if owner.heap != self.heap.identity || owner.execution != self.execution {
            return Err(Error::ForeignOwner);
        }
        self.owners.get(owner.index).ok_or(Error::ForeignOwner)?;
        Ok(owner.index)
    }

    pub(super) fn finalizer_roots(finalizer: Finalizer, roots: &mut Vec<Value>) {
        match finalizer {
            Finalizer::Task { task: r, services }
            | Finalizer::WithCause { func: r, services }
            | Finalizer::WithExit { func: r, services } => {
                Self::reg_into_roots(r, roots);
                roots.extend(services);
            }
            Finalizer::Release {
                value,
                release_fn,
                services,
            } => {
                roots.push(value);
                Self::reg_into_roots(release_fn, roots);
                roots.extend(services);
            }
            Finalizer::CloseChildOwner(_) | Finalizer::ReleaseMemo { .. } => {}
        }
    }

    pub(super) fn exec_scope_run(&mut self, dst: usize, body_slot: usize) -> Result<(), Error> {
        let body = self.take_reg(body_slot)?;
        let previous = self.current_owner;
        let owner = self.owners.len();
        self.owners.push(OwnerState::default());
        self.current_owner = owner;
        let handle = self.owner_handle(owner);
        self.current_evidence = Some(OwnerEvidence::from_owner(&handle));
        self.apply_reg(
            body,
            RegVal::Value(Value::Owner(handle)),
            End::CloseScope {
                owner,
                previous,
                dst: Some(dst),
            },
            false,
        )
    }

    pub(super) fn exec_scope_current(&mut self, dst: usize, unit_slot: usize) -> Result<(), Error> {
        let unit = self.take_reg(unit_slot)?;
        self.drop_reg(unit)?;
        let handle = Value::Owner(self.owner_handle(self.current_owner));
        self.store_raw(dst, RegVal::Value(handle));
        Ok(())
    }

    pub(super) fn exec_scope_add_finalizer(
        &mut self,
        dst: usize,
        task_slot: usize,
    ) -> Result<(), Error> {
        let task = self.take_reg(task_slot)?;
        let services = self.snapshot_active_services()?;
        self.register_scope_finalizer(dst, Finalizer::Task { task, services })
    }

    pub(super) fn exec_scope_add_finalizer_exit(
        &mut self,
        dst: usize,
        func_slot: usize,
    ) -> Result<(), Error> {
        let func = self.take_reg(func_slot)?;
        let services = self.snapshot_active_services()?;
        self.register_scope_finalizer(dst, Finalizer::WithCause { func, services })
    }

    pub(super) fn exec_scope_add_close(
        &mut self,
        dst: usize,
        func_slot: usize,
    ) -> Result<(), Error> {
        let func = self.take_reg(func_slot)?;
        let services = self.snapshot_active_services()?;
        self.register_scope_finalizer(dst, Finalizer::WithExit { func, services })
    }

    fn register_scope_finalizer(&mut self, dst: usize, finalizer: Finalizer) -> Result<(), Error> {
        let owner = self.current_owner;
        if self.owners[owner].is_closed() {
            Self::finalizer_roots(finalizer, &mut self.scratch);
            return self.raise_defect(DefectSpec::arena_owner_closed());
        }
        if self.owners[owner].is_finalizing() {
            let exit_cause = match &self.owners[owner].exit_cause {
                Some(c) => Some(self.heap.dup_cause(c)?),
                None => None,
            };
            let saved_mask = self.mask_depth;
            self.mask_depth += 1;
            let next = End::RestoreMask {
                saved_mask,
                check_on_exit: true,
                next: Box::new(End::Store(dst)),
            };
            let res = self.run_single_finalizer(finalizer, exit_cause.as_ref(), next);
            if let Some(c) = exit_cause {
                self.heap.drop_cause(c)?;
            }
            return res;
        }
        self.owners[owner].finalizers.push(finalizer);
        self.store_raw(dst, RegVal::Value(Value::Unit));
        Ok(())
    }

    pub(super) fn owner_needs_async_close(&self, owner: usize) -> bool {
        let o = &self.owners[owner];
        !o.is_closed()
            && (!o.hooks.is_empty()
                || !o.children.is_empty()
                || o.watchers > 0
                || !o.finalizers.is_empty())
    }

    pub(super) fn close_owner(&mut self, owner: usize) -> Result<(), Error> {
        let state = &mut self.owners[owner];
        if state.is_closed() {
            return Ok(());
        }
        state.phase = OwnerPhase::Closed;
        state.hooks.clear();
        state.children.clear();
        state.close_waiter = None;
        let mut roots = mem::take(&mut state.arenas);
        let exit_cause = state.exit_cause.take();
        let finalizers = mem::take(&mut state.finalizers);
        let mut detached = Vec::new();
        let mut failure = None;
        for root in &roots {
            let Value::Object(token) = root else {
                failure.get_or_insert(Error::ExpectedArena);
                continue;
            };
            match self.heap.object_mut(token.id).map(|obj| &mut obj.data) {
                Ok(Data::Arena { contents, .. }) => detached.append(contents),
                Ok(_) => {
                    failure.get_or_insert(Error::ExpectedArena);
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        if let Some(c) = exit_cause {
            roots.push(c.into_value());
        }
        for fin in finalizers {
            Self::finalizer_roots(fin, &mut roots);
        }
        roots.extend(detached);
        let released = self.heap.drop_values(roots);
        failure.map_or(released, Err)
    }
}
