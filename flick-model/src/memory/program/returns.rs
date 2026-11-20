impl<'p> Machine<'p, '_> {
    fn returned_frame(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::RecoverySelection { id } => self.complete_recovery(id)?,
            Destination::RecoveryHelper { id, binding } => {
                let value = self.pending.take().expect("helper result");
                self.store_transport(binding, value, id)?;
            }
            Destination::RecoveryBody {
                binding,
                owner,
                evidence,
            } => {
                self.current_owner = owner;
                self.current_evidence = evidence;
                let value = self.pending.take().expect("selected body result");
                self.store(binding, value)?;
            }
            Destination::LayerRun { provider, binding } => {
                self.leave_provider(provider)?;
                let value = self.pending.take().expect("layer result");
                self.store(binding, value)?;
            }
            Destination::Handle {
                activation,
                binding,
            } => self.handler_returned(activation, binding)?,
            Destination::Discontinue { cont_id, binding } => {
                let value = self.pending.take().expect("control arm result");
                if let Some(Some(captured)) = self.continuations.get_mut(&cont_id).map(Option::take)
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
            Destination::Invocation(id) => {
                self.invocations
                    .get_mut(&id)
                    .expect("running invocation")
                    .callee = self.pending.take();
                self.ready = Some(id);
            }
            Destination::Catch { handler, binding } => {
                self.pending_handler = Some(handler);
                self.heap
                    .drop_value(self.pending_handler.take().expect("unused handler"))?;
                let value = self.pending.take().expect("try result");
                self.store(binding, value)?;
            }
            Destination::Initialize { name, binding } => {
                let value = self.pending.take().expect("returned token");
                self.globals.insert(name.clone(), Some(value));
                let root = self.globals[&name].as_ref().expect("initialized global");
                let owned = self.heap.dup(root)?;
                self.deliver(&binding.map_or(Destination::Top, Destination::Bind), owned)?;
            }
            Destination::Modify { cell, binding } => {
                self.complete_modify(&cell, binding)?;
            }
            Destination::CloseScope {
                owner,
                previous,
                binding,
            } => {
                let value = self.pending.take().expect("scope result");
                self.begin_close_scope(owner, previous, ScopeOutcome::Normal { binding, value })?;
            }
            Destination::ClosingScope { state } => {
                let unit = self.pending.take().expect("finalizer return");
                self.heap.drop_value(unit)?;
                self.step_close_scope(state)?;
            }
            Destination::RestoreOwner { previous, binding } => {
                self.current_owner = previous;
                let value = self.pending.take().expect("selected-owner result");
                self.store(binding, value)?;
            }
            destination => self.returned_control_destination(destination)?,
        }
        Ok(())
    }

    fn returned_control_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::Loop {
                id,
                code,
                args,
                binding,
            } => {
                let ignored = self.pending.take().expect("loop iteration result");
                self.heap.drop_value(ignored)?;
                for arg in &args {
                    match self.heap.dup(arg) {
                        Ok(dup) => self.scratch.push(dup),
                        Err(err) => {
                            self.heap.drop_values(args)?;
                            return Err(err);
                        }
                    }
                }
                self.call(
                    code,
                    Destination::Loop {
                        id,
                        code,
                        args,
                        binding,
                    },
                )?;
            }
            Destination::FunctionBoundary { binding } => {
                let value = self.pending.take().expect("function boundary result");
                self.store(binding, value)?;
            }
            Destination::GenerateStrict { items, binding, .. } => {
                let ignored = self.pending.take().expect("strict generator return");
                self.heap.drop_value(ignored)?;
                let list = self.heap.construct("$list", items)?;
                self.store(binding, list)?;
            }
            Destination::GeneratorStep { gen_id, binding } => {
                let ignored = self.pending.take().expect("lazy generator return");
                self.heap.drop_value(ignored)?;
                if let Some(slot) = self.generators.get_mut(&gen_id) {
                    slot.run = GeneratorRun::Done;
                }
                let none = self.heap.construct("Prelude.Option.None", vec![])?;
                self.store(binding, none)?;
            }
            Destination::LazyComplete { .. }
            | Destination::AcquireRegister { .. }
            | Destination::BracketUse { .. }
            | Destination::BracketRelease { .. }
            | Destination::BracketFinish { .. } => {
                self.returned_resource_destination(destination)?;
            }
            Destination::DiscardFinalizer => {
                let ignored = self.pending.take().expect("discarded result");
                self.heap.drop_value(ignored)?;
            }
            destination => {
                let value = self.pending.take().expect("returned token");
                self.deliver(&destination, value)?;
            }
        }
        Ok(())
    }

    fn returned_resource_destination(&mut self, destination: Destination<'p>) -> Result<(), Error> {
        match destination {
            Destination::LazyComplete { lazy, binding } => {
                let value = self.pending.take().expect("lazy result");
                let cached = match self.heap.dup(&value) {
                    Ok(dup) => dup,
                    Err(err) => {
                        self.heap.drop_value(value)?;
                        return Err(err);
                    }
                };
                let slot = self.heap.lazy_state_mut(self.execution, &lazy)?;
                let prev = slot.replace(LazyState::DoneOk(cached));
                if let Some(LazyState::Running(task)) = prev {
                    self.heap.drop_value(task)?;
                }
                self.store(binding, value)?;
            }
            Destination::AcquireRegister {
                owner,
                release,
                binding,
            } => {
                let value = self.pending.take().expect("acquired resource");
                let dup = match self.heap.dup(&value) {
                    Ok(dup) => dup,
                    Err(err) => {
                        self.heap.drop_values(vec![value, release])?;
                        return Err(err);
                    }
                };
                self.owners[owner].finalizers.push(Finalizer::Release {
                    value: dup,
                    release_fn: release,
                });
                self.store(binding, value)?;
            }
            Destination::BracketUse {
                use_fn,
                release_fn,
                binding,
            } => {
                let resource = self.pending.take().expect("bracket acquired resource");
                let saved = match self.heap.dup(&resource) {
                    Ok(dup) => dup,
                    Err(err) => {
                        self.heap.drop_values(vec![resource, use_fn, release_fn])?;
                        return Err(err);
                    }
                };
                self.scratch.extend([use_fn, resource]);
                self.apply(Destination::BracketRelease {
                    resource: saved,
                    release_fn,
                    binding,
                })?;
            }
            Destination::BracketRelease {
                resource,
                release_fn,
                binding,
            } => {
                let value = self.pending.take().expect("bracket use result");
                self.scratch.extend([release_fn, resource]);
                self.apply(Destination::BracketFinish {
                    outcome: BracketOutcome::Success(value),
                    binding,
                })?;
            }
            Destination::BracketFinish { outcome, binding } => {
                let ignored = self.pending.take().expect("bracket release result");
                self.heap.drop_value(ignored)?;
                match outcome {
                    BracketOutcome::Success(value) => self.store(binding, value)?,
                    BracketOutcome::Failure(cause) => {
                        self.failure = Some(cause);
                        self.failure_elected = false;
                    }
                }
            }
            _ => unreachable!("non-resource destination"),
        }
        Ok(())
    }
}
