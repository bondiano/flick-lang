//! Complete providers retain caller evidence and own a real lexical scope.
use super::{Destination, Invocation, InvocationEnd, Machine, OwnerState};
use crate::memory::{Data, Error, Heap, LayerData, Value};
use flick_core::ir::memory::{Intrinsic, Local};
use std::mem;

pub(super) struct Provider {
    pub(super) layer: Value,
    pub(super) owner: usize,
    pub(super) previous: usize,
}

impl Machine<'_, '_> {
    pub(super) fn layer_intrinsic(&mut self, kind: Intrinsic, binding: Local) -> Result<(), Error> {
        match kind {
            Intrinsic::LayerSucceed => {
                let Value::Object(tag) = &self.scratch[0] else {
                    return Err(Error::ExpectedServiceTag);
                };
                if !matches!(self.heap.object(tag.id)?.data, Data::ServiceTag { .. }) {
                    return Err(Error::ExpectedServiceTag);
                }
                let mut inputs = mem::take(&mut self.scratch).into_iter();
                let value = self.heap.allocate(Data::Layer(LayerData::Succeed {
                    tag: inputs.next().expect("tag"),
                    implementation: inputs.next().expect("implementation"),
                }));
                self.store(binding, value)?;
            }
            Intrinsic::LayerMerge => {
                layer_data(self.heap, &self.scratch[0])?;
                layer_data(self.heap, &self.scratch[1])?;
                let mut inputs = mem::take(&mut self.scratch).into_iter();
                let value = self.heap.allocate(Data::Layer(LayerData::Merge {
                    left: inputs.next().expect("left layer"),
                    right: inputs.next().expect("right layer"),
                }));
                self.store(binding, value)?;
            }
            Intrinsic::LayerRun => {
                layer_data(self.heap, &self.scratch[0])?;
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
                    },
                );
                self.scratch.push(Value::Unit);
                self.apply(Destination::LayerRun {
                    provider: id,
                    binding,
                })?;
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
        Ok(implementation(heap, &provider.layer, effect, fuel)?.is_some())
    }

    pub(super) fn invoke_provider(
        &mut self,
        id: usize,
        binding: Local,
        effect: &str,
        op: &str,
    ) -> Result<(), Error> {
        let implementation = implementation(
            self.heap,
            &self.providers[&id].layer,
            effect,
            &mut self.fuel,
        )?
        .expect("selected provider service");
        let function = self.heap.named_field(implementation, op)?.representation();
        let function = self.heap.dup(&function)?;
        self.scratch.insert(0, function);
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
        layer_data(self.heap, self.top.as_ref().expect("root layer"))?;
        let id = self.handler_identity()?;
        let layer = self.top.take().expect("root layer");
        let owner = self.current_owner;
        self.providers.insert(
            id,
            Provider {
                layer,
                owner,
                previous: owner,
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

    pub(super) fn leave_provider(&mut self, id: usize) -> Result<(), Error> {
        let provider = self.providers.remove(&id).expect("departed provider");
        self.scratch.push(provider.layer);
        self.close_owner(provider.owner)?;
        self.current_owner = provider.previous;
        self.heap
            .drop_value(self.scratch.pop().expect("provider layer"))
    }
}

// All currently representable constructors are pure graph nodes. Adding dynamic
// acquisition requires an actual build phase; it must not be approximated by
// this lookup or by installing nested providers with different owners/evidence.
fn layer_data<'a>(heap: &'a Heap, value: &Value) -> Result<&'a LayerData, Error> {
    let Value::Object(value) = value else {
        return Err(Error::ExpectedLayer);
    };
    let Data::Layer(layer) = &heap.object(value.id)?.data else {
        return Err(Error::ExpectedLayer);
    };
    Ok(layer)
}

fn implementation<'a>(
    heap: &'a Heap,
    layer: &'a Value,
    effect: &str,
    fuel: &mut u64,
) -> Result<Option<&'a Value>, Error> {
    let mut pending = vec![layer];
    while let Some(layer) = pending.pop() {
        *fuel = fuel.checked_sub(1).ok_or(Error::OutOfFuel)?;
        match layer_data(heap, layer)? {
            LayerData::Merge { left, right } => {
                // Native servicesOf processes left-to-right with replacement;
                // searching right-first gives the same complete-provider key.
                pending.extend([left, right]);
            }
            LayerData::Succeed {
                tag,
                implementation,
            } => {
                let Value::Object(tag) = tag else {
                    return Err(Error::ExpectedServiceTag);
                };
                let Data::ServiceTag { key } = &heap.object(tag.id)?.data else {
                    return Err(Error::ExpectedServiceTag);
                };
                if key == effect {
                    return Ok(Some(implementation));
                }
            }
        }
    }
    Ok(None)
}
