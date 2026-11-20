impl Machine<'_, '_> {
    fn owner_handle(&self, index: usize) -> OwnerHandle {
        OwnerHandle {
            heap: self.heap.identity,
            execution: self.execution,
            index,
        }
    }

    fn owner_index(&self, value: &Value) -> Result<usize, Error> {
        let Value::Owner(owner) = value else {
            return Err(Error::ExpectedOwner);
        };
        if owner.heap != self.heap.identity || owner.execution != self.execution {
            return Err(Error::ForeignOwner);
        }
        // Selection carries identity; resource admission checks retirement.
        self.owners.get(owner.index).ok_or(Error::ForeignOwner)?;
        Ok(owner.index)
    }

    /// Detach every arena slot and unrun finalizer before dropping owners.
    fn close_owner(&mut self, owner: usize) -> Result<(), Error> {
        let state = &mut self.owners[owner];
        if state.closed {
            return Ok(());
        }
        state.closed = true;
        state.closing = false;
        let mut roots = mem::take(&mut state.arenas);
        let finalizers = mem::take(&mut state.finalizers);
        let mut detached = Vec::new();
        let mut failure = None;
        for root in &roots {
            let Value::Object(token) = root else {
                failure.get_or_insert(Error::ExpectedArena);
                continue;
            };
            match self
                .heap
                .object_mut(token.id)
                .map(|object| &mut object.data)
            {
                Ok(Data::Arena { contents, .. }) => detached.append(contents),
                Ok(_) => {
                    failure.get_or_insert(Error::ExpectedArena);
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        for fin in finalizers {
            Self::finalizer_roots(fin, &mut roots);
        }
        roots.extend(detached);
        let released = self.heap.drop_values(roots);
        failure.map_or(released, Err)
    }

    /// Only owned runtime roots are drained. The heap's occupied slots are
    /// never scanned to choose objects for disposal.
    fn cleanup(&mut self) -> Result<(), Error> {
        let mut roots = mem::take(&mut self.scratch);
        roots.extend(self.pending.take());
        if let Some(cause) = self.failure.take() {
            roots.push(cause.into_value());
        }
        roots.extend(self.pending_handler.take());
        roots.extend(self.top.take());
        roots.extend(self.arguments.drain(..));
        let active_frames = mem::take(&mut self.frames);
        for frame in active_frames {
            self.drain_frame_roots(frame, &mut roots);
        }
        self.cleanup_handlers(&mut roots);
        for slot in mem::take(&mut self.generators).into_values() {
            match slot.run {
                GeneratorRun::Fresh { seq, .. } => roots.push(seq),
                GeneratorRun::Suspended { frames, .. } => {
                    for frame in frames {
                        self.drain_frame_roots(frame, &mut roots);
                    }
                }
                GeneratorRun::Running | GeneratorRun::Done => {}
            }
        }
        self.cleanup_recoveries(&mut roots);
        for provider in mem::take(&mut self.providers).into_values() {
            roots.push(provider.layer);
        }
        roots.extend(mem::take(&mut self.globals).into_values().flatten());
        // Scope arenas and any unrun finalizers close even on fuel exhaustion.
        let mut failure = None;
        for owner in (0..self.owners.len()).rev() {
            if let Err(error) = self.close_owner(owner) {
                failure.get_or_insert(error);
            }
        }
        let mut detached = Vec::new();
        // Detach ALL cell and lazy slots before releasing any of their values.
        // This walks only our explicit roots, never heap reachability.
        for root in &self.cells {
            let Value::Object(token) = root else {
                failure.get_or_insert(Error::ExpectedCell);
                continue;
            };
            match self
                .heap
                .object_mut(token.id)
                .map(|object| &mut object.data)
            {
                Ok(Data::Cell { contents, .. }) => detached.extend(contents.take()),
                Ok(_) => {
                    failure.get_or_insert(Error::ExpectedCell);
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        for root in &self.lazies {
            let Value::Object(token) = root else {
                failure.get_or_insert(Error::ExpectedLazy);
                continue;
            };
            match self
                .heap
                .object_mut(token.id)
                .map(|object| &mut object.data)
            {
                Ok(Data::Lazy { state, .. }) => {
                    if let Some(prev) = state.take() {
                        match prev {
                            LazyState::Pending(v)
                            | LazyState::Running(v)
                            | LazyState::DoneOk(v)
                            | LazyState::DoneErr(v) => detached.push(v),
                        }
                    }
                }
                Ok(_) => {
                    failure.get_or_insert(Error::ExpectedLazy);
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        roots.extend(mem::take(&mut self.cells));
        roots.extend(mem::take(&mut self.lazies));
        roots.extend(detached);
        let released = self.heap.drop_values(roots);
        let _ = self.heap.take_dropped_continuations(self.execution);
        failure.map_or(released, Err)
    }
}
