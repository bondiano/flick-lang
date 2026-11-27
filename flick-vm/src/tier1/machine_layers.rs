//! Layer providers, `Context.*` / `Layer.*` / manual-scope / env-capture
//! intrinsics, and service lookup for tier 1.

use std::collections::BTreeMap;
use std::mem;

use flick_core::ir::memory::LayerIntrinsic;

use crate::stdlib::FuelCounter;
use crate::stdlib::runtime::coop::MEMO_REF_KEY;
use crate::stdlib::runtime::{context as std_context, layers as std_layers};
use crate::tier0::{CauseToken, Data, DefectSpec, Error, Heap, Value};

use super::machine::{End, Machine, OwnerEvidence, RegVal};
use super::machine_handlers::{Invocation, InvocationEnd};
use super::machine_scopes::{OwnerState, ScopeOutcome};

pub(super) enum MemoRestore {
    Unchanged,
    Restore(Option<Value>),
}

pub(super) struct Provider {
    pub(super) layer: Value,
    pub(super) owner: usize,
    pub(super) previous: usize,
    pub(super) restore_memo: MemoRestore,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum LayerBuildTarget {
    Local(usize),
    Top,
    Provide(usize),
}

pub(super) struct PendingProvide {
    pub(super) owner: usize,
    pub(super) previous: usize,
    pub(super) body_thunk: Value,
    pub(super) saved_evidence: Option<OwnerEvidence>,
    pub(super) prev_memo: Option<Value>,
    pub(super) dst: usize,
}

pub(super) enum PendingProvideResult {
    Ready(usize, Value),
    Failed(usize, CauseToken),
}

impl Machine<'_, '_> {
    pub(super) fn exec_layer_succeed(
        &mut self,
        dst: usize,
        tag: usize,
        value: usize,
    ) -> Result<(), Error> {
        self.stage_slots(&[tag, value])?;
        let out = std_layers::layer_succeed(self.heap, &mut self.scratch)?;
        self.store_reg(dst, RegVal::Value(out))
    }

    pub(super) fn exec_layer_merge(
        &mut self,
        dst: usize,
        left: usize,
        right: usize,
    ) -> Result<(), Error> {
        self.stage_slots(&[left, right])?;
        let out = std_layers::layer_merge(self.heap, &mut self.scratch)?;
        self.store_reg(dst, RegVal::Value(out))
    }

    pub(super) fn exec_layer_run(
        &mut self,
        dst: usize,
        layer_slot: usize,
        body_slot: usize,
    ) -> Result<(), Error> {
        self.stage_slots(&[layer_slot, body_slot])?;
        std_layers::ensure_layer(self.heap, &self.scratch[0])?;
        if std_layers::layer_is_static(self.heap, &self.scratch[0])? {
            let id = self.handler_identity()?;
            let previous = self.current_owner;
            let owner = self.owners.len();
            self.owners.push(OwnerState::default());
            self.current_owner = owner;
            let layer = self.scratch.remove(0);
            let body = self.scratch.pop().expect("layer body thunk");
            self.providers.insert(
                id,
                Provider {
                    layer,
                    owner,
                    previous,
                    restore_memo: MemoRestore::Unchanged,
                },
            );
            self.bump_handler_epoch();
            self.apply_reg(
                RegVal::Value(body),
                RegVal::Value(Value::Unit),
                End::LayerRun { provider: id, dst },
                false,
            )
        } else {
            let body_thunk = self.scratch.pop().expect("layer body thunk");
            let layer = self.scratch.pop().expect("dynamic layer");
            self.begin_dynamic_layer_run(layer, body_thunk, dst)
        }
    }

    pub(super) fn exec_layer_op(
        &mut self,
        dst: usize,
        op: LayerIntrinsic,
        args: &[usize],
    ) -> Result<(), Error> {
        self.stage_slots(args)?;
        match op {
            LayerIntrinsic::ContextEmpty
            | LayerIntrinsic::ContextMake
            | LayerIntrinsic::ContextAdd
            | LayerIntrinsic::ContextAddOrOmit
            | LayerIntrinsic::ContextGet
            | LayerIntrinsic::ContextGetOption
            | LayerIntrinsic::ContextMerge
            | LayerIntrinsic::ContextMergeAll
            | LayerIntrinsic::ContextPick
            | LayerIntrinsic::ContextOmit
            | LayerIntrinsic::ContextReference => {
                match std_context::eval_pure_context(self.heap, op, &mut self.scratch)? {
                    std_context::ContextOutcome::Value(out) => {
                        self.store_reg(dst, RegVal::Value(out))
                    }
                    std_context::ContextOutcome::Defect(spec) => self.raise_defect(spec),
                }
            }
            LayerIntrinsic::ContextRead => self.exec_context_read(dst),
            LayerIntrinsic::ContextWithReference => self.exec_context_with_ref(dst),
            LayerIntrinsic::LayerEmpty
            | LayerIntrinsic::LayerSucceedContext
            | LayerIntrinsic::LayerFromBuild
            | LayerIntrinsic::LayerFresh
            | LayerIntrinsic::LayerOrDie
            | LayerIntrinsic::LayerMergeAll => {
                let val = std_layers::eval_layer_ctor(self.heap, op, &mut self.scratch)?;
                self.store_reg(dst, RegVal::Value(val))
            }
            LayerIntrinsic::LayerMakeMemoMap
            | LayerIntrinsic::LayerForkMemoMap
            | LayerIntrinsic::LayerCurrentMemoMap
            | LayerIntrinsic::LayerBuild
            | LayerIntrinsic::LayerBuildWithMemoMap => self.layer_memo_intrinsic(op, dst),
            LayerIntrinsic::LayerProvideContext => {
                let task = self.scratch.pop().expect("task");
                let ctx = self.scratch.pop().expect("context");
                std_context::expect_context_map(self.heap, &ctx)?;
                self.bump_handler_epoch();
                self.await_task_reg(
                    RegVal::Value(task),
                    End::ProvideContext { context: ctx, dst },
                )
            }
            LayerIntrinsic::LayerLookupService => {
                let tag_val = self.scratch.pop().expect("service tag");
                let key = std_context::service_tag_key(self.heap, &tag_val)?.to_owned();
                self.heap.drop_value(tag_val)?;
                if let Some(impl_val) = self.lookup_service_impl(&key)? {
                    return self.store_reg(dst, RegVal::Value(impl_val));
                }
                self.raise_defect(DefectSpec::custom(format!("Service not found: {key}")))
            }
            LayerIntrinsic::OwnerNew
            | LayerIntrinsic::OwnerRun
            | LayerIntrinsic::OwnerClose
            | LayerIntrinsic::EnvCapture
            | LayerIntrinsic::EnvRun => self.manual_scope_intrinsic(op, dst),
            LayerIntrinsic::ContextSetFiberRef => self.exec_context_set_fiber_ref(dst),
            LayerIntrinsic::MetricRuntimeFibersStarted => {
                self.release_operands()?;
                #[expect(clippy::cast_precision_loss, reason = "fiber count fits exact f64")]
                let count = self.runtime_fibers_started as f64;
                self.store_reg(dst, RegVal::Value(Value::Float(count)))
            }
        }
    }

    fn exec_context_read(&mut self, dst: usize) -> Result<(), Error> {
        let ref_val = self.scratch.pop().expect("context ref");
        let (key, default_rep) = {
            let Value::Object(tok) = &ref_val else {
                self.heap.drop_value(ref_val)?;
                return Err(Error::ExpectedContextRef);
            };
            let Data::ContextRef { key, default_fn } = &self.heap.object(tok.id)?.data else {
                self.heap.drop_value(ref_val)?;
                return Err(Error::ExpectedContextRef);
            };
            (key.clone(), default_fn.representation())
        };
        match self.lookup_service_impl(&key) {
            Ok(Some(bound)) => {
                self.heap.drop_value(ref_val)?;
                return self.store_reg(dst, RegVal::Value(bound));
            }
            Ok(None) => {}
            Err(err) => {
                self.heap.drop_value(ref_val)?;
                return Err(err);
            }
        }
        if let Some(bound) = self.fiber_refs.get(&key) {
            let rep = bound.representation();
            let dup = self.heap.dup(&rep)?;
            self.heap.drop_value(ref_val)?;
            return self.store_reg(dst, RegVal::Value(dup));
        }
        let default_fn = self.heap.dup(&default_rep)?;
        self.heap.drop_value(ref_val)?;
        self.apply_reg(
            RegVal::Value(default_fn),
            RegVal::Value(Value::Unit),
            End::Store(dst),
            false,
        )
    }

    fn exec_context_set_fiber_ref(&mut self, dst: usize) -> Result<(), Error> {
        let val = self.scratch.pop().expect("fiber ref val");
        let ref_val = self.scratch.pop().expect("context ref");
        let Value::Object(tok) = &ref_val else {
            self.heap.drop_values(vec![ref_val, val])?;
            return Err(Error::ExpectedContextRef);
        };
        let Data::ContextRef { key, .. } = &self.heap.object(tok.id)?.data else {
            self.heap.drop_values(vec![ref_val, val])?;
            return Err(Error::ExpectedContextRef);
        };
        let key = key.clone();
        self.heap.drop_value(ref_val)?;
        if let Some(old) = self.fiber_refs.insert(key, val) {
            self.heap.drop_value(old)?;
        }
        self.store_reg(dst, RegVal::Value(Value::Unit))
    }

    fn exec_context_with_ref(&mut self, dst: usize) -> Result<(), Error> {
        let task = self.scratch.pop().expect("task");
        let val = self.scratch.pop().expect("value");
        let ref_val = self.scratch.pop().expect("ref");
        let Value::Object(tok) = &ref_val else {
            self.heap.drop_values(vec![ref_val, val, task])?;
            return Err(Error::ExpectedContextRef);
        };
        let Data::ContextRef { key, .. } = &self.heap.object(tok.id)?.data else {
            self.heap.drop_values(vec![ref_val, val, task])?;
            return Err(Error::ExpectedContextRef);
        };
        let key = key.clone();
        self.heap.drop_value(ref_val)?;
        if key.starts_with("flick/Scheduler/")
            || key == MEMO_REF_KEY
            || key == "effect/References/CurrentLogAnnotations"
        {
            let prev = self.fiber_refs.insert(key.clone(), val);
            return self.await_task_reg(
                RegVal::Value(task),
                End::RestoreFiberRef {
                    key,
                    previous: prev,
                    dst,
                },
            );
        }
        let mut map = BTreeMap::new();
        map.insert(key, val);
        let ctx = self.heap.allocate(Data::Context(map));
        self.bump_handler_epoch();
        self.await_task_reg(
            RegVal::Value(task),
            End::ProvideContext { context: ctx, dst },
        )
    }

    pub(super) fn install_root(&mut self) -> Result<(), Error> {
        let reg = self.top.take().expect("root layer");
        let layer = self.materialize_reg(reg);
        if let Err(err) = std_layers::ensure_layer(self.heap, &layer) {
            self.heap.drop_value(layer)?;
            return Err(err);
        }
        let id = match self.handler_identity() {
            Ok(id) => id,
            Err(err) => {
                self.heap.drop_value(layer)?;
                return Err(err);
            }
        };
        let owner = self.current_owner;
        self.providers.insert(
            id,
            Provider {
                layer,
                owner,
                previous: owner,
                restore_memo: MemoRestore::Unchanged,
            },
        );
        self.root = Some(id);
        self.bump_handler_epoch();
        Ok(())
    }

    pub(super) fn root_with(&mut self, effect: &str) -> Result<Option<usize>, Error> {
        let Some(root) = self.root else {
            return Ok(None);
        };
        let matched = std_layers::lookup_layer_service(
            self.heap,
            &self.providers[&root].layer,
            effect,
            &mut FuelCounter(&mut self.fuel),
        )?
        .is_some();
        Ok(matched.then_some(root))
    }

    pub(super) fn invoke_provider(
        &mut self,
        id: usize,
        dst: usize,
        effect: &str,
        op: &str,
    ) -> Result<(), Error> {
        let implementation = std_layers::lookup_layer_service(
            self.heap,
            &self.providers[&id].layer,
            effect,
            &mut FuelCounter(&mut self.fuel),
        )?
        .expect("selected provider service");
        let function = self.heap.named_field(implementation, op)?.representation();
        let function = self.heap.dup(&function)?;
        self.queue_service_invocation(RegVal::Value(function), dst)
    }

    pub(super) fn invoke_service_impl(
        &mut self,
        impl_rep: &Value,
        dst: usize,
        op: &str,
    ) -> Result<(), Error> {
        let function = self.heap.named_field(impl_rep, op)?.representation();
        let function = self.heap.dup(&function)?;
        self.queue_service_invocation(RegVal::Value(function), dst)
    }

    pub(super) fn queue_service_invocation(
        &mut self,
        callee: RegVal,
        dst: usize,
    ) -> Result<(), Error> {
        let invocation = match self.handler_identity() {
            Ok(id) => id,
            Err(err) => {
                self.drop_reg(callee)?;
                return Err(err);
            }
        };
        let args = mem::take(&mut self.scratch)
            .into_iter()
            .map(RegVal::Value)
            .collect();
        self.invocations.insert(
            invocation,
            Invocation {
                evidence: self.current_evidence,
                callee: Some(callee),
                arguments: args,
                end: InvocationEnd::Caller(dst),
            },
        );
        self.ready = Some(invocation);
        Ok(())
    }

    pub(super) fn leave_provider_outcome(
        &mut self,
        id: usize,
        outcome: ScopeOutcome,
    ) -> Result<(), Error> {
        let provider = self.providers.remove(&id).expect("departed provider");
        self.bump_handler_epoch();
        if let MemoRestore::Restore(prev_memo) = provider.restore_memo {
            self.restore_fiber_memo(prev_memo)?;
        }
        let owner = provider.owner;
        let previous = provider.previous;
        if self.owner_needs_async_close(owner) {
            self.heap.drop_value(provider.layer)?;
            return self.begin_close_scope(owner, previous, outcome, None);
        }
        self.scratch.push(provider.layer);
        self.close_owner(owner)?;
        self.current_owner = previous;
        self.heap
            .drop_value(self.scratch.pop().expect("provider layer"))?;
        self.complete_scope_outcome(outcome, None, false)
    }

    pub(super) fn find_end_service(
        heap: &Heap,
        end: &End,
        providers: &BTreeMap<usize, Provider>,
        effect: &str,
        fuel: &mut u64,
    ) -> Result<Option<Value>, Error> {
        match end {
            End::LayerRun { provider, .. } => {
                if let Some(p) = providers.get(provider)
                    && let Some(imp) = std_layers::lookup_layer_service(
                        heap,
                        &p.layer,
                        effect,
                        &mut FuelCounter(fuel),
                    )?
                {
                    return Ok(Some(imp.representation()));
                }
                Ok(None)
            }
            End::ProvideContext { context, .. } => {
                if let Some(imp) =
                    std_layers::lookup_layer_service(heap, context, effect, &mut FuelCounter(fuel))?
                {
                    return Ok(Some(imp.representation()));
                }
                Ok(None)
            }
            End::WithServices { services, next } => {
                for s in services {
                    if let Some(imp) =
                        std_layers::lookup_layer_service(heap, s, effect, &mut FuelCounter(fuel))?
                    {
                        return Ok(Some(imp.representation()));
                    }
                }
                Self::find_end_service(heap, next, providers, effect, fuel)
            }
            End::RestoreMask { next, .. } | End::CachedComplete { next, .. } => {
                Self::find_end_service(heap, next, providers, effect, fuel)
            }
            _ => Ok(None),
        }
    }

    pub(super) fn lookup_service_impl(&mut self, effect: &str) -> Result<Option<Value>, Error> {
        let mut found_rep = None;
        for frame in self.frames.iter().rev() {
            if let Some(rep) = Self::find_end_service(
                self.heap,
                &frame.end,
                &self.providers,
                effect,
                &mut self.fuel,
            )? {
                found_rep = Some(rep);
                break;
            }
        }
        if found_rep.is_none() {
            for s in &self.inherited_services {
                if let Some(imp) = std_layers::lookup_layer_service(
                    self.heap,
                    s,
                    effect,
                    &mut FuelCounter(&mut self.fuel),
                )? {
                    found_rep = Some(imp.representation());
                    break;
                }
            }
        }
        if found_rep.is_none()
            && let Some(root) = self.root
            && let Some(imp) = std_layers::lookup_layer_service(
                self.heap,
                &self.providers[&root].layer,
                effect,
                &mut FuelCounter(&mut self.fuel),
            )?
        {
            found_rep = Some(imp.representation());
        }
        found_rep.map(|r| self.heap.dup(&r)).transpose()
    }
}
