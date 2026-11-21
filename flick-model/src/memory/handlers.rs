//! Explicit owned handler activations, parked frames and metered transfers.
use super::{
    Activation, BracketOutcome, CapturedContinuation, Data, Destination, Discard, Error,
    GeneratorRun, InstalledOperation, Invocation, InvocationEnd, Local, Machine, Operation,
    ScopeOutcome, Segment, TransferNext, Value, charge, iter, mem,
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

    pub(super) fn transfer_fuel(&mut self, frames: usize) -> Result<(), Error> {
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
            if let Some(impl_rep) = Self::find_dest_service(
                self.heap,
                &self.frames[at].destination,
                &self.providers,
                effect,
                &mut self.fuel,
            )? {
                return self.invoke_service_impl(&impl_rep, binding, op);
            }
            if let Destination::Handle {
                activation,
                binding,
            } = self.frames[at].destination
            {
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
        }
        let Some((at, result_binding, outer_owner, operation)) = selected else {
            return self.perform_fallback(binding, effect, op, dicts.len(), args.len());
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
        let class = operation.class;
        let fn_rep = operation.function.representation();
        if class == OpClass::Control && self.maybe_cutoff_control(at)? {
            return Ok(());
        }
        let function = self.heap.dup(&fn_rep)?;
        self.scratch.insert(0, function);
        self.dispatch_handler_operation(at, binding, result_binding, outer_owner, class)
    }

    fn perform_fallback(
        &mut self,
        binding: Local,
        effect: &str,
        op: &str,
        dict_count: usize,
        arg_count: usize,
    ) -> Result<(), Error> {
        let mut inherited_rep = None;
        for s in &self.inherited_services {
            if let Some(imp) =
                super::layers::lookup_layer_service(self.heap, s, effect, &mut self.fuel)?
            {
                inherited_rep = Some(imp.representation());
                break;
            }
        }
        if let Some(rep) = inherited_rep {
            return self.invoke_service_impl(&rep, binding, op);
        }
        let inherited_op = self
            .inherited_tail_ops
            .iter()
            .find(|candidate| candidate.effect == effect && candidate.name == op)
            .map(|candidate| {
                (
                    candidate.dicts,
                    candidate.arity,
                    candidate.function.representation(),
                )
            });
        if let Some((expected_dicts, expected_args, fn_rep)) = inherited_op {
            if expected_dicts != dict_count || expected_args != arg_count {
                return Err(Error::OperationArgumentCount {
                    effect: effect.into(),
                    op: op.into(),
                    expected_dicts,
                    actual_dicts: dict_count,
                    expected_args,
                    actual_args: arg_count,
                });
            }
            let function = self.heap.dup(&fn_rep)?;
            self.scratch.insert(0, function);
            return self.apply(Destination::Bind(binding));
        }
        if let Some(root) = self.root_with(effect)? {
            return self.invoke_provider(root, binding, effect, op);
        }
        charge(
            &mut self.fuel,
            effect.len().saturating_add(op.len()).saturating_add(1),
        )?;
        Err(Error::MissingHandler {
            effect: effect.into(),
            op: op.into(),
        })
    }

    fn maybe_cutoff_control(&mut self, at: usize) -> Result<bool, Error> {
        let mut cutoff_at = None;
        for idx in (at + 1..self.frames.len()).rev() {
            if self.is_cutoff_cleanup_destination(&self.frames[idx].destination)? {
                cutoff_at = Some(idx);
                break;
            }
        }
        let Some(idx) = cutoff_at else {
            return Ok(false);
        };
        self.release_operands()?;
        self.transfer_fuel(self.frames.len() - (idx + 1))?;
        let cleanup_frames = self.frames.split_off(idx + 1);
        self.discard = Some(Discard {
            frames: cleanup_frames,
            owner: Some(self.current_owner),
            next: TransferNext::FinishCutoff,
        });
        Ok(true)
    }

    fn dispatch_handler_operation(
        &mut self,
        at: usize,
        binding: Local,
        result_binding: Local,
        outer_owner: usize,
        class: OpClass,
    ) -> Result<(), Error> {
        self.transfer_fuel(self.frames.len() - at)?;
        let segment_id = self.handler_identity()?;
        let frames = self.frames.split_off(at);
        let old_owner = self.current_owner;
        self.current_owner = outer_owner;
        let end = match class {
            OpClass::Tail => {
                self.segments.insert(
                    segment_id,
                    Segment {
                        frames,
                        owner: old_owner,
                        binding,
                    },
                );
                InvocationEnd::Tail(segment_id)
            }
            OpClass::Abort => {
                self.discard = Some(Discard {
                    frames,
                    owner: Some(outer_owner),
                    next: TransferNext::Raise,
                });
                InvocationEnd::Abort(result_binding)
            }
            OpClass::Control => {
                self.continuations.insert(
                    segment_id,
                    Some(CapturedContinuation {
                        frames,
                        owner: old_owner,
                        binding,
                    }),
                );
                let cont = self.heap.allocate(Data::Continuation {
                    execution: self.execution,
                    id: segment_id,
                });
                self.scratch.push(cont);
                InvocationEnd::Control {
                    cont_id: segment_id,
                    binding: result_binding,
                }
            }
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
        if matches!(class, OpClass::Tail | OpClass::Control) {
            self.ready = Some(invocation);
        } else {
            self.discard.as_mut().expect("abort transfer").next = TransferNext::Invoke(invocation);
        }
        Ok(())
    }

    pub(super) fn invoke_step(&mut self, id: usize) -> Result<(), Error> {
        let invocation = self.invocations.get_mut(&id).expect("pending invocation");
        self.current_evidence = invocation.evidence;
        if invocation.arguments.len() == 1
            && matches!(invocation.end, InvocationEnd::Control { .. })
        {
            let mut invocation = self.invocations.remove(&id).expect("final invocation step");
            self.scratch
                .push(invocation.callee.take().expect("invocation function"));
            self.scratch
                .push(invocation.arguments.pop_front().expect("final argument"));
            let InvocationEnd::Control { cont_id, binding } = invocation.end else {
                unreachable!("control invocation matched above")
            };
            return self.apply(Destination::Discontinue { cont_id, binding });
        }
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
                InvocationEnd::Control { cont_id, binding } => {
                    self.restore_evidence();
                    let value = self.pending.take().expect("control arm value");
                    if let Some(Some(captured)) =
                        self.continuations.get_mut(&cont_id).map(Option::take)
                    {
                        self.discard = Some(Discard {
                            frames: captured.frames,
                            owner: Some(self.current_owner),
                            next: TransferNext::DeliverValue { binding, value },
                        });
                    } else {
                        self.store(binding, value)?;
                    }
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
        match invocation.end {
            InvocationEnd::Tail(id) => {
                let segment = self.segments.remove(&id).expect("failed tail segment");
                self.discard = Some(Discard {
                    frames: segment.frames,
                    owner: Some(self.current_owner),
                    next: TransferNext::Raise,
                });
            }
            InvocationEnd::Control { cont_id, .. } => {
                if let Some(Some(captured)) = self.continuations.get_mut(&cont_id).map(Option::take)
                {
                    let cause = self.failure.take();
                    self.discard = Some(Discard {
                        frames: captured.frames,
                        owner: Some(self.current_owner),
                        next: TransferNext::ResumeRunning {
                            saved_failure: cause,
                        },
                    });
                }
            }
            InvocationEnd::Caller(_) | InvocationEnd::Abort(_) => {}
        }
        Ok(())
    }

    pub(super) fn cleanup_handlers(&mut self, roots: &mut Vec<Value>) {
        for activation in mem::take(&mut self.activations).into_values() {
            Self::activation_roots(activation, roots);
        }
        for segment in mem::take(&mut self.segments).into_values() {
            for frame in segment.frames {
                self.drain_frame_roots(frame, roots);
            }
        }
        for captured in mem::take(&mut self.continuations).into_values().flatten() {
            for frame in captured.frames {
                self.drain_frame_roots(frame, roots);
            }
        }
        if let Some(discard) = self.discard.take() {
            self.drain_discard_roots(discard, roots);
        }
        for invocation in mem::take(&mut self.invocations).into_values() {
            roots.extend(invocation.callee);
            roots.extend(invocation.arguments);
        }
    }
}

include!("handlers_discard.rs");
