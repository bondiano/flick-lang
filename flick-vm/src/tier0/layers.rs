//! Complete providers retain caller evidence and own a real lexical scope.
use std::mem;

use flick_core::ir::memory::{Intrinsic, Local};

use crate::stdlib::FuelCounter;
use crate::stdlib::runtime::layers as std_layers;
use crate::tier0::{Data, Error, Heap, Value};

use super::{Destination, Invocation, InvocationEnd, Machine, OwnerState, ScopeOutcome};

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

impl Machine<'_, '_> {
    pub(super) fn layer_intrinsic(&mut self, kind: Intrinsic, binding: Local) -> Result<(), Error> {
        match kind {
            Intrinsic::LayerSucceed => {
                let value = std_layers::layer_succeed(self.heap, &mut self.scratch)?;
                self.store(binding, value)?;
            }
            Intrinsic::LayerMerge => {
                let value = std_layers::layer_merge(self.heap, &mut self.scratch)?;
                self.store(binding, value)?;
            }
            Intrinsic::LayerRun => {
                std_layers::ensure_layer(self.heap, &self.scratch[0])?;
                if std_layers::layer_is_static(self.heap, &self.scratch[0])? {
                    let id = self.handler_identity()?;
                    let previous = self.current_owner;
                    let owner = self.owners.len();
                    self.owners.push(OwnerState::default());
                    self.current_owner = owner;
                    let layer = self.scratch.remove(0);
                    self.providers.insert(
                        id,
                        Provider {
                            layer,
                            owner,
                            previous,
                            restore_memo: MemoRestore::Unchanged,
                        },
                    );
                    self.scratch.push(Value::Unit);
                    self.apply(Destination::LayerRun {
                        provider: id,
                        binding,
                    })?;
                } else {
                    let body_thunk = self.scratch.pop().expect("layer body thunk");
                    let layer = self.scratch.pop().expect("dynamic layer");
                    self.begin_dynamic_layer_run(layer, body_thunk, binding)?;
                }
            }
            _ => unreachable!("layer intrinsic"),
        }
        Ok(())
    }

    pub(super) fn provider_matches(
        heap: &Heap,
        provider: &Provider,
        effect: &str,
        fuel: &mut u64,
    ) -> Result<bool, Error> {
        Ok(
            std_layers::lookup_layer_service(
                heap,
                &provider.layer,
                effect,
                &mut FuelCounter(fuel),
            )?
            .is_some(),
        )
    }

    pub(super) fn invoke_provider(
        &mut self,
        id: usize,
        binding: Local,
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
        self.scratch.insert(0, function);
        self.queue_service_invocation(binding)
    }

    pub(super) fn invoke_service_impl(
        &mut self,
        impl_rep: &Value,
        binding: Local,
        op: &str,
    ) -> Result<(), Error> {
        if let Value::Object(tok) = impl_rep
            && let Data::ForeignHandle { execution, host_id } = self.heap.object(tok.id)?.data
        {
            return self.foreign_invoke_service_op(binding, execution, host_id, op);
        }
        let function = self.heap.named_field(impl_rep, op)?.representation();
        let function = self.heap.dup(&function)?;
        self.scratch.insert(0, function);
        self.queue_service_invocation(binding)
    }

    fn queue_service_invocation(&mut self, binding: Local) -> Result<(), Error> {
        // Reserve identity while every owned argument is still rooted.
        let invocation = self.handler_identity()?;
        let mut operands = mem::take(&mut self.scratch).into_iter();
        self.invocations.insert(
            invocation,
            Invocation {
                evidence: self.current_evidence,
                callee: operands.next(),
                arguments: operands.collect(),
                end: InvocationEnd::Caller(binding),
            },
        );
        self.ready = Some(invocation);
        Ok(())
    }

    /// `Σ₀`: the layer the root global gave becomes a provider that no
    /// frame holds. It answers what no frame does until cleanup releases it.
    pub(super) fn install_root(&mut self) -> Result<(), Error> {
        std_layers::ensure_layer(self.heap, self.top.as_ref().expect("root layer"))?;
        let id = self.handler_identity()?;
        let layer = self.top.take().expect("root layer");
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
        Ok(())
    }

    /// The root provider, when it has `effect`.
    pub(super) fn root_with(&mut self, effect: &str) -> Result<Option<usize>, Error> {
        let Some(root) = self.root else {
            return Ok(None);
        };
        let provider = &self.providers[&root];
        Ok(Self::provider_matches(self.heap, provider, effect, &mut self.fuel)?.then_some(root))
    }

    pub(super) fn leave_provider_outcome(
        &mut self,
        id: usize,
        outcome: ScopeOutcome,
    ) -> Result<(), Error> {
        let provider = self.providers.remove(&id).expect("departed provider");
        if let MemoRestore::Restore(prev_memo) = provider.restore_memo {
            self.restore_fiber_memo(prev_memo)?;
        }
        let owner = provider.owner;
        let previous = provider.previous;
        if self.owner_needs_async_close(owner) {
            self.heap.drop_value(provider.layer)?;
            return self.begin_close_scope(owner, previous, outcome);
        }
        self.scratch.push(provider.layer);
        self.close_mirrored_owner(owner)?;
        self.close_owner(owner)?;
        self.current_owner = previous;
        self.heap
            .drop_value(self.scratch.pop().expect("provider layer"))?;
        self.complete_scope_outcome(outcome, None, false)
    }
}

pub(super) fn lookup_layer_service<'a>(
    heap: &'a Heap,
    value: &'a Value,
    effect: &str,
    fuel: &mut u64,
) -> Result<Option<&'a Value>, Error> {
    std_layers::lookup_layer_service(heap, value, effect, &mut FuelCounter(fuel))
}
