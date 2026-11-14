//! Explicit owned handler activations, parked frames and metered transfers.
use super::{
    Activation, Destination, Discard, Error, InstalledOperation, Invocation, InvocationEnd, Local,
    Machine, Operation, Segment, TransferNext, Value, charge, iter, mem,
};
use flick_core::ir::memory::HandlerEffect;
use flick_core::types::OpClass;

impl Machine<'_, '_> {
    pub(super) fn handler_operation(
        &mut self,
        binding: Local,
        operation: &Operation,
    ) -> Result<bool, Error> {
        match operation {
            Operation::ServiceTag(key) => {
                let value = self.heap.service_tag(key)?;
                self.store(binding, value)?;
            }
            Operation::Handle {
                body,
                effects,
                ret,
                errors,
            } => self.install_handler(binding, *body, effects, *ret, *errors)?,
            Operation::Perform {
                effect,
                op,
                dicts,
                args,
            } => self.perform_handler(binding, effect, op, dicts, args)?,
            _ => return Ok(false),
        }
        Ok(true)
    }
    pub(super) fn handler_identity(&mut self) -> Result<usize, Error> {
        let id = self.next_handler;
        self.next_handler = id.checked_add(1).ok_or(Error::HandlerIdentityOverflow)?;
        Ok(id)
    }

    fn transfer_fuel(&mut self, frames: usize) -> Result<(), Error> {
        let amount = u64::try_from(frames).map_err(|_| Error::OutOfFuel)?;
        self.fuel = self.fuel.checked_sub(amount).ok_or(Error::OutOfFuel)?;
        Ok(())
    }

    pub(super) fn install_handler(
        &mut self,
        binding: Local,
        body: Local,
        effects: &[HandlerEffect],
        ret: Option<Local>,
        errors: Option<Local>,
    ) -> Result<(), Error> {
        if effects
            .iter()
            .flat_map(|e| &e.operations)
            .any(|op| op.class == OpClass::Control)
        {
            return Err(Error::UnsupportedOperation("control handler"));
        }
        self.stage(
            iter::once(body)
                .chain(
                    effects
                        .iter()
                        .flat_map(|e| e.operations.iter().map(|op| op.function)),
                )
                .chain(ret)
                .chain(errors),
        )?;
        let id = self.handler_identity()?;
        let mut operands = mem::take(&mut self.scratch).into_iter();
        self.scratch.push(operands.next().expect("handler body"));
        let operations = effects
            .iter()
            .flat_map(|effect| {
                effect
                    .operations
                    .iter()
                    .map(|op| InstalledOperation {
                        effect: effect.effect.clone(),
                        name: op.name.clone(),
                        class: op.class,
                        dicts: op.dicts,
                        arity: op.arity,
                        function: operands.next().expect("handler operation"),
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let activation = Activation {
            operations,
            ret: ret.map(|_| operands.next().expect("return arm")),
            errors: errors.map(|_| operands.next().expect("error dispatcher")),
            owner: self.current_owner,
        };
        self.activations.insert(id, activation);
        self.scratch.push(Value::Unit);
        self.apply(Destination::Handle {
            activation: id,
            binding,
        })
    }

    pub(super) fn perform_handler(
        &mut self,
        binding: Local,
        effect: &str,
        op: &str,
        dicts: &[Local],
        args: &[Local],
    ) -> Result<(), Error> {
        self.stage(dicts.iter().chain(args).copied())?;
        let mut selected = None;
        for at in (0..self.frames.len()).rev() {
            match self.frames[at].destination {
                Destination::LayerRun { provider, .. }
                    if Self::provider_matches(
                        self.heap,
                        &self.providers[&provider],
                        effect,
                        &mut self.fuel,
                    )? =>
                {
                    return self.invoke_provider(provider, binding, effect, op);
                }
                Destination::Handle {
                    activation,
                    binding,
                } => {
                    let handler = &self.activations[&activation];
                    if let Some(operation) = handler
                        .operations
                        .iter()
                        .find(|operation| operation.effect == effect && operation.name == op)
                    {
                        selected = Some((at, binding, handler.owner, operation));
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some((at, result_binding, outer_owner, operation)) = selected else {
            charge(
                &mut self.fuel,
                effect.len().saturating_add(op.len()).saturating_add(1),
            )?;
            return Err(Error::MissingHandler {
                effect: effect.into(),
                op: op.into(),
            });
        };
        if operation.dicts != dicts.len() || operation.arity != args.len() {
            return Err(Error::OperationArgumentCount {
                effect: effect.into(),
                op: op.into(),
                expected_dicts: operation.dicts,
                actual_dicts: dicts.len(),
                expected_args: operation.arity,
                actual_args: args.len(),
            });
        }
        if operation.class == OpClass::Control {
            return Err(Error::UnsupportedOperation("control operation"));
        }
        let class = operation.class;
        let function = self.heap.dup(&operation.function)?;
        self.scratch.insert(0, function);
        self.transfer_fuel(self.frames.len() - at)?;
        let segment_id = self.handler_identity()?;
        let frames = self.frames.split_off(at);
        let old_owner = self.current_owner;
        self.current_owner = outer_owner;
        let end = if class == OpClass::Tail {
            self.segments.insert(
                segment_id,
                Segment {
                    frames,
                    owner: old_owner,
                    binding,
                },
            );
            InvocationEnd::Tail(segment_id)
        } else {
            self.discard = Some(Discard {
                frames,
                owner: outer_owner,
                next: TransferNext::Raise,
            });
            InvocationEnd::Abort(result_binding)
        };
        let invocation = self.handler_identity()?;
        let mut operands = mem::take(&mut self.scratch).into_iter();
        self.invocations.insert(
            invocation,
            Invocation {
                callee: operands.next(),
                arguments: operands.collect(),
                end,
                evidence: self.current_evidence,
            },
        );
        if class == OpClass::Tail {
            self.ready = Some(invocation);
        } else {
            self.discard.as_mut().expect("abort transfer").next = TransferNext::Invoke(invocation);
        }
        Ok(())
    }

    pub(super) fn invoke_step(&mut self, id: usize) -> Result<(), Error> {
        let invocation = self.invocations.get_mut(&id).expect("pending invocation");
        self.current_evidence = invocation.evidence;
        if let Some(argument) = invocation.arguments.pop_front() {
            self.scratch
                .push(invocation.callee.take().expect("invocation function"));
            self.scratch.push(argument);
            self.apply(Destination::Invocation(id))
        } else {
            self.pending = invocation.callee.take();
            let invocation = self.invocations.remove(&id).expect("completed invocation");
            match invocation.end {
                InvocationEnd::Tail(segment) => {
                    let count = self.segments[&segment].frames.len();
                    self.transfer_fuel(count)?;
                    let segment = self.segments.remove(&segment).expect("parked tail body");
                    self.current_owner = segment.owner;
                    self.frames.extend(segment.frames);
                    self.restore_evidence();
                    let value = self.pending.take().expect("tail arm value");
                    self.store(segment.binding, value)?;
                }
                InvocationEnd::Caller(binding) | InvocationEnd::Abort(binding) => {
                    self.restore_evidence();
                    let value = self.pending.take().expect("abort arm value");
                    self.store(binding, value)?;
                }
            }
            Ok(())
        }
    }

    fn activation_roots(activation: Activation, roots: &mut Vec<Value>) {
        roots.extend(activation.operations.into_iter().map(|op| op.function));
        roots.extend(activation.ret);
        roots.extend(activation.errors);
    }

    pub(super) fn handler_returned(&mut self, id: usize, binding: Local) -> Result<(), Error> {
        let mut activation = self.activations.remove(&id).expect("completed handler");
        self.current_owner = activation.owner;
        self.pending_handler = activation.ret.take();
        let mut roots = Vec::new();
        Self::activation_roots(activation, &mut roots);
        self.heap.drop_values(roots)?;
        let value = self.pending.take().expect("handler body value");
        if let Some(ret) = self.pending_handler.take() {
            self.scratch.extend([ret, value]);
            self.apply(Destination::Bind(binding))
        } else {
            self.store(binding, value)?;
            Ok(())
        }
    }

    pub(super) fn handler_failed(&mut self, id: usize, binding: Local) -> Result<(), Error> {
        let mut activation = self.activations.remove(&id).expect("failed handler");
        self.current_owner = activation.owner;
        self.pending_handler = activation.errors.take();
        let mut roots = Vec::new();
        Self::activation_roots(activation, &mut roots);
        self.heap.drop_values(roots)?;
        if self.pending_handler.is_some() {
            self.begin_recovery(binding)?;
        }
        Ok(())
    }

    pub(super) fn invocation_failed(&mut self, id: usize) -> Result<(), Error> {
        let invocation = self.invocations.remove(&id).expect("failed invocation");
        let mut roots: Vec<_> = invocation.arguments.into_iter().collect();
        roots.extend(invocation.callee);
        self.heap.drop_values(roots)?;
        if let InvocationEnd::Tail(id) = invocation.end {
            let segment = self.segments.remove(&id).expect("failed tail segment");
            self.discard = Some(Discard {
                frames: segment.frames,
                owner: self.current_owner,
                next: TransferNext::Raise,
            });
        }
        Ok(())
    }

    /// Internal abort/discontinuation cannot be intercepted by ordinary Try.
    pub(super) fn discard_step(&mut self) -> Result<(), Error> {
        let frame = self.discard.as_mut().expect("active transfer").frames.pop();
        let Some(frame) = frame else {
            let discard = self.discard.take().expect("completed transfer");
            self.current_owner = discard.owner;
            if let TransferNext::Invoke(id) = discard.next {
                self.ready = Some(id);
            }
            return Ok(());
        };
        self.heap
            .drop_values(frame.locals.into_values().flatten().collect())?;
        let mut roots = Vec::new();
        match frame.destination {
            Destination::RecoverySelection { id } => self.abandon_recovery(id)?,
            Destination::RecoveryBody {
                owner, evidence, ..
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
            }
            Destination::LayerRun { provider, .. } => self.leave_provider(provider)?,
            Destination::Handle { activation, .. } => {
                Self::activation_roots(
                    self.activations
                        .remove(&activation)
                        .expect("departed handler"),
                    &mut roots,
                );
            }
            Destination::Invocation(id) => {
                let invocation = self.invocations.remove(&id).expect("departed invocation");
                roots.extend(invocation.callee);
                roots.extend(invocation.arguments);
                if let InvocationEnd::Tail(id) = invocation.end {
                    let segment = self.segments.remove(&id).expect("departed tail segment");
                    self.discard
                        .as_mut()
                        .expect("active transfer")
                        .frames
                        .extend(segment.frames);
                }
            }
            Destination::Catch { handler, .. } => roots.push(handler),
            Destination::Initialize { name, .. } => {
                self.globals.remove(&name);
            }
            Destination::CloseScope {
                owner, previous, ..
            } => {
                self.close_owner(owner)?;
                self.current_owner = previous;
            }
            Destination::RestoreOwner { previous, .. } => self.current_owner = previous,
            Destination::RecoveryHelper { .. }
            | Destination::Top
            | Destination::Bind(_)
            | Destination::Modify { .. } => {}
        }
        self.heap.drop_values(roots)
    }

    pub(super) fn cleanup_handlers(&mut self, roots: &mut Vec<Value>) {
        for activation in mem::take(&mut self.activations).into_values() {
            Self::activation_roots(activation, roots);
        }
        let mut frames = Vec::new();
        for segment in mem::take(&mut self.segments).into_values() {
            frames.extend(segment.frames);
        }
        if let Some(discard) = self.discard.take() {
            frames.extend(discard.frames);
        }
        for frame in frames {
            roots.extend(frame.locals.into_values().flatten());
            if let Destination::Catch { handler, .. } = frame.destination {
                roots.push(handler);
            }
        }
        for invocation in mem::take(&mut self.invocations).into_values() {
            roots.extend(invocation.callee);
            roots.extend(invocation.arguments);
        }
    }
}
