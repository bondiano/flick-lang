// Frame-resource release, root draining, and continuation tombstone cleanup.
impl<'p> Machine<'p, '_> {
    fn drain_transfer_next_roots(&mut self, next: &mut TransferNext<'p>, roots: &mut Vec<Value>) {
        let old = mem::replace(next, TransferNext::Raise);
        match old {
            TransferNext::DeliverValue { value, .. } => roots.push(value),
            TransferNext::LoopContinue { args, .. } => roots.extend(args),
            TransferNext::FinishGenerateStrict { items, .. } => roots.extend(items),
            TransferNext::ResumeRunning { saved_failure, .. } => {
                if let Some(cause) = saved_failure {
                    roots.push(cause.into_value());
                }
            }
            TransferNext::Chain(chained) => self.drain_discard_roots(*chained, roots),
            TransferNext::Invoke(id) => {
                if let Some(inv) = self.invocations.remove(&id) {
                    roots.extend(inv.callee);
                    roots.extend(inv.arguments);
                }
            }
            TransferNext::Raise
            | TransferNext::ReturnNone { .. }
            | TransferNext::DeliverNone { .. }
            | TransferNext::FinishCutoff => {}
        }
    }

    fn drain_discard_roots(&mut self, mut discard: Discard<'p>, roots: &mut Vec<Value>) {
        for frame in discard.frames.drain(..) {
            self.drain_frame_roots(frame, roots);
        }
        self.drain_transfer_next_roots(&mut discard.next, roots);
    }

    fn drain_closing_scope_roots(&mut self, state: ClosingScopeState<'p>, roots: &mut Vec<Value>) {
        for fin in state.remaining {
            Self::finalizer_roots(fin, roots);
        }
        if let Some(cause) = state.exit_override {
            roots.push(cause.into_value());
        }
        if let Some(cause) = state.finalizer_failure {
            roots.push(cause.into_value());
        }
        match state.outcome {
            ScopeOutcome::Normal { value, .. } => roots.push(value),
            ScopeOutcome::Failure(cause) => roots.push(cause.into_value()),
            ScopeOutcome::Discard(discard) => self.drain_discard_roots(*discard, roots),
            ScopeOutcome::ChildClose {
                parent,
                inherited_exit,
            } => {
                if let Some(c) = inherited_exit {
                    roots.push(c.into_value());
                }
                self.drain_closing_scope_roots(*parent, roots);
            }
        }
    }

    fn drain_destination_roots(&mut self, destination: Destination<'p>, roots: &mut Vec<Value>) {
        match destination {
            Destination::Catch { handler, .. }
            | Destination::CatchDefect { handler, .. }
            | Destination::CatchCause { handler, .. } => {
                roots.push(handler);
            }
            Destination::Loop { args, .. } => roots.extend(args),
            Destination::GenerateStrict { items, .. } => roots.extend(items),
            Destination::AcquireRegister { release, .. } => roots.push(release),
            Destination::BracketUse {
                use_fn, release_fn, ..
            } => {
                roots.push(use_fn);
                roots.push(release_fn);
            }
            Destination::BracketRelease {
                resource,
                release_fn,
                ..
            } => {
                roots.push(resource);
                roots.push(release_fn);
            }
            Destination::BracketFinish { outcome, .. }
            | Destination::EnsuringDone { outcome, .. }
            | Destination::OnExitRunFin { outcome, .. }
            | Destination::OnExitFinish { outcome, .. } => match outcome {
                BracketOutcome::Success(v) => roots.push(v),
                BracketOutcome::Failure(cause) => roots.push(cause.into_value()),
            },
            Destination::ClosingScope { state } => self.drain_closing_scope_roots(*state, roots),
            Destination::DiscardCleanupDone { paused, .. }
            | Destination::OnExitDiscardRunFin { paused, .. } => {
                self.drain_discard_roots(*paused, roots);
            }
            Destination::RestoreMask { next, .. } | Destination::CachedComplete { next, .. } => {
                self.drain_destination_roots(*next, roots);
            }
            Destination::EnsuringFinish { finalizer, .. } => roots.push(finalizer),
            Destination::OnExitDecide { decide, .. } => roots.push(decide),
            Destination::ProvideContext { context, .. } => roots.push(context),
            Destination::WithServices { services, next } => {
                roots.extend(services);
                self.drain_destination_roots(*next, roots);
            }
            Destination::RestoreFiberRef { previous, .. } => roots.extend(previous),
            Destination::RestoreEnvRefs { previous, .. } => roots.extend(previous.into_values()),
            Destination::LayerBuildFinish { prev_memo, .. } => roots.extend(prev_memo),
            Destination::HostSyncCallback { remaining_args, .. }
            | Destination::HostApplyChain { remaining_args } => {
                roots.extend(remaining_args);
            }
            Destination::OrDie { .. }
            | Destination::RestoreOwnerEvidence { .. }
            | Destination::TimedFinish { .. }
            | Destination::RestoreRandom { .. }
            | Destination::RecoverySelection { .. }
            | Destination::RecoveryHelper { .. }
            | Destination::RecoveryBody { .. }
            | Destination::LayerRun { .. }
            | Destination::Handle { .. }
            | Destination::Discontinue { .. }
            | Destination::Invocation(_)
            | Destination::Top
            | Destination::Bind(_)
            | Destination::Initialize { .. }
            | Destination::Modify { .. }
            | Destination::CloseScope { .. }
            | Destination::RestoreOwner { .. }
            | Destination::FunctionBoundary { .. }
            | Destination::GeneratorStep { .. }
            | Destination::LazyComplete { .. }
            | Destination::CaptureExit { .. }
            | Destination::RaiseFromCatch
            | Destination::ForeignLayerBuildFinish { .. }
            | Destination::DiscardFinalizer => {}
        }
    }

    fn drain_frame_roots(&mut self, mut frame: Frame<'p>, roots: &mut Vec<Value>) {
        frame.locals.drain_values(roots);
        for root in &frame.vars {
            if let Value::Object(token) = root
                && let Ok(object) = self.heap.object_mut(token.id)
                && let Data::Var { contents, .. } = &mut object.data
            {
                roots.extend(contents.take());
            }
        }
        roots.extend(mem::take(&mut frame.vars));
        for root in &frame.iterators {
            if let Value::Object(token) = root
                && let Ok(object) = self.heap.object_mut(token.id)
                && let Data::Iterator { state, .. } = &mut object.data
            {
                let prev = mem::replace(state, IteratorState::Exhausted);
                match prev {
                    IteratorState::List { backing, .. } => roots.push(backing),
                    IteratorState::Generator { id } => {
                        if let Some(slot) = self.generators.remove(&id) {
                            match slot.run {
                                GeneratorRun::Fresh { seq, .. } => roots.push(seq),
                                GeneratorRun::Suspended { frames, .. } => {
                                    for gen_frame in frames {
                                        self.drain_frame_roots(gen_frame, roots);
                                    }
                                }
                                GeneratorRun::Running | GeneratorRun::Done => {}
                            }
                        }
                    }
                    IteratorState::Exhausted => {}
                }
            }
        }
        roots.extend(mem::take(&mut frame.iterators));
        self.drain_destination_roots(frame.destination, roots);
    }

    fn release_frame_resources(&mut self, frame: &mut Frame<'p>) -> Result<Vec<Frame<'p>>, Error> {
        let mut roots = Vec::new();
        for root in &frame.vars {
            if let Value::Object(token) = root
                && let Ok(object) = self.heap.object_mut(token.id)
                && let Data::Var { contents, .. } = &mut object.data
            {
                roots.extend(contents.take());
            }
        }
        roots.extend(mem::take(&mut frame.vars));
        let mut gen_frames = Vec::new();
        for root in &frame.iterators {
            if let Value::Object(token) = root
                && let Ok(object) = self.heap.object_mut(token.id)
                && let Data::Iterator { state, .. } = &mut object.data
            {
                let prev = mem::replace(state, IteratorState::Exhausted);
                match prev {
                    IteratorState::List { backing, .. } => roots.push(backing),
                    IteratorState::Generator { id } => {
                        if let Some(slot) = self.generators.remove(&id) {
                            match slot.run {
                                GeneratorRun::Fresh { seq, .. } => roots.push(seq),
                                GeneratorRun::Suspended { frames, .. } => {
                                    gen_frames.extend(frames);
                                }
                                GeneratorRun::Running | GeneratorRun::Done => {}
                            }
                        }
                    }
                    IteratorState::Exhausted => {}
                }
            }
        }
        roots.extend(mem::take(&mut frame.iterators));
        self.heap.drop_values(roots)?;
        Ok(gen_frames)
    }

    fn queue_discard_frames(&mut self, frames: Vec<Frame<'p>>) {
        if frames.is_empty() {
            return;
        }
        let next = if let Some(prev) = self.discard.take() {
            TransferNext::Chain(Box::new(prev))
        } else {
            let saved_failure = self.failure.take();
            if saved_failure.is_some() {
                self.failure_elected = false;
            }
            TransferNext::ResumeRunning {
                saved_failure,
                propagate_exit: false,
            }
        };
        self.discard = Some(Discard {
            frames,
            owner: Some(self.current_owner),
            next,
            aborted: false,
        });
    }

    /// Reclaims map tombstones for already-consumed or already-discontinued continuations.
    /// Live unresumed continuations are discontinued by `Destination::Discontinue` when
    /// their control arm exits, preserving the exact discontinuation timing.
    fn drain_dropped_continuations(&mut self) {
        let dropped = self.heap.take_dropped_continuations(self.execution);
        for id in dropped {
            if matches!(self.continuations.get(&id), Some(None)) {
                self.continuations.remove(&id);
            }
        }
    }

    fn finalizer_roots(finalizer: Finalizer, roots: &mut Vec<Value>) {
        match finalizer {
            Finalizer::Task { task: v, services }
            | Finalizer::WithCause { func: v, services }
            | Finalizer::WithExit { func: v, services } => {
                roots.push(v);
                roots.extend(services);
            }
            Finalizer::Release {
                value,
                release_fn,
                services,
            } => {
                roots.push(value);
                roots.push(release_fn);
                roots.extend(services);
            }
            Finalizer::CloseChildOwner(_) | Finalizer::ReleaseMemo { .. } => {}
        }
    }
}
