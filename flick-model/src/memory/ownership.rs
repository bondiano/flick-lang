impl Heap {
    pub fn dup(&mut self, value: &Value) -> Result<Value, Error> {
        if let Value::Object(id) = value {
            let object = self.object_mut(id.id)?;
            object.references = object
                .references
                .checked_add(1)
                .ok_or(Error::ReferenceOverflow(id.id))?;
            self.stats.dups += 1;
        }
        Ok(value.representation())
    }

    /// Releases one token and iteratively destroys any newly unreachable
    /// descendants. There is no recursive Rust drop chain or tracing pass.
    pub fn drop_value(&mut self, value: Value) -> Result<(), Error> {
        self.drop_values(vec![value])
    }

    fn allocate(&mut self, data: Data) -> Value {
        self.stats.cells_allocated += usize::from(matches!(data, Data::Cell { .. }));
        self.stats.arenas_allocated += usize::from(matches!(data, Data::Arena { .. }));
        let (index, generation) = if let Some(index) = self.free_list.pop() {
            let slot = &mut self.objects[index];
            slot.object = Some(Object {
                references: 1,
                data,
            });
            (index, slot.generation)
        } else {
            let index = self.objects.len();
            self.objects.push(Slot {
                generation: 0,
                object: Some(Object {
                    references: 1,
                    data,
                }),
            });
            (index, 0)
        };
        let id = ObjectId {
            heap: self.identity,
            index,
            generation,
        };
        self.stats.allocations += 1;
        self.stats.live_objects += 1;
        self.stats.peak_live_objects = self.stats.peak_live_objects.max(self.stats.live_objects);
        Value::Object(ObjectToken { id })
    }

    fn object(&self, id: ObjectId) -> Result<&Object, Error> {
        if id.heap != self.identity {
            return Err(Error::InvalidObject(id));
        }
        self.objects
            .get(id.index)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.object.as_ref())
            .ok_or(Error::InvalidObject(id))
    }

    fn object_mut(&mut self, id: ObjectId) -> Result<&mut Object, Error> {
        if id.heap != self.identity {
            return Err(Error::InvalidObject(id));
        }
        self.objects
            .get_mut(id.index)
            .filter(|slot| slot.generation == id.generation)
            .and_then(|slot| slot.object.as_mut())
            .ok_or(Error::InvalidObject(id))
    }

    fn cell_contents<'a>(
        &'a self,
        execution: usize,
        cell: &CellHandle,
    ) -> Result<&'a Value, Error> {
        if cell.execution != execution {
            return Err(Error::ForeignCell);
        }
        let Data::Cell {
            execution: owner,
            contents,
        } = &self.object(cell.id)?.data
        else {
            return Err(Error::ExpectedCell);
        };
        if *owner != execution {
            return Err(Error::ForeignCell);
        }
        contents.as_ref().ok_or(Error::ClosedCell)
    }

    fn cell_contents_mut<'a>(
        &'a mut self,
        execution: usize,
        cell: &CellHandle,
    ) -> Result<&'a mut Value, Error> {
        if cell.execution != execution {
            return Err(Error::ForeignCell);
        }
        let Data::Cell {
            execution: owner,
            contents,
        } = &mut self.object_mut(cell.id)?.data
        else {
            return Err(Error::ExpectedCell);
        };
        if *owner != execution {
            return Err(Error::ForeignCell);
        }
        contents.as_mut().ok_or(Error::ClosedCell)
    }

    fn arena_contents<'a>(
        &'a self,
        execution: usize,
        arena: &ArenaHandle,
    ) -> Result<&'a Vec<Value>, Error> {
        if arena.execution != execution || arena.id.heap != self.identity {
            return Err(Error::ForeignArena);
        }
        // VM-minted generational ids distinguish retired handles from never-allocated slots.
        let slot = self
            .objects
            .get(arena.id.index)
            .ok_or(Error::InvalidObject(arena.id))?;
        let object = slot
            .object
            .as_ref()
            .filter(|_| slot.generation == arena.id.generation)
            .ok_or(Error::ClosedArena)?;
        let Data::Arena {
            execution: owner,
            contents,
        } = &object.data
        else {
            return Err(Error::ExpectedArena);
        };
        if *owner != execution {
            return Err(Error::ForeignArena);
        }
        Ok(contents)
    }

    fn arena_contents_mut<'a>(
        &'a mut self,
        execution: usize,
        arena: &ArenaHandle,
    ) -> Result<&'a mut Vec<Value>, Error> {
        if arena.execution != execution || arena.id.heap != self.identity {
            return Err(Error::ForeignArena);
        }
        let slot = self
            .objects
            .get_mut(arena.id.index)
            .ok_or(Error::InvalidObject(arena.id))?;
        if slot.generation != arena.id.generation {
            return Err(Error::ClosedArena);
        }
        let object = slot.object.as_mut().ok_or(Error::ClosedArena)?;
        let Data::Arena {
            execution: owner,
            contents,
        } = &mut object.data
        else {
            return Err(Error::ExpectedArena);
        };
        if *owner != execution {
            return Err(Error::ForeignArena);
        }
        Ok(contents)
    }

    fn admit_ordinary(&self, value: &Value) -> Result<(), Error> {
        match value {
            Value::ReuseNull => return Err(Error::ReuseTokenEscape),
            Value::Object(token) => match self.object(token.id)?.data {
                Data::RecoveryResult { .. } => return Err(Error::RecoveryEscape),
                Data::ReuseToken { .. } => return Err(Error::ReuseTokenEscape),
                _ => {}
            },
            _ => {}
        }
        Ok(())
    }

    fn admit_reuse_candidate(&self, value: &Value) -> Result<(), Error> {
        if let Value::Object(token) = value
            && matches!(self.object(token.id)?.data, Data::RecoveryResult { .. })
        {
            return Err(Error::RecoveryEscape);
        }
        Ok(())
    }

    fn admit_not_reuse_token(&self, value: &Value) -> Result<(), Error> {
        match value {
            Value::ReuseNull => Err(Error::ReuseTokenEscape),
            Value::Object(token)
                if matches!(self.object(token.id)?.data, Data::ReuseToken { .. }) =>
            {
                Err(Error::ReuseTokenEscape)
            }
            _ => Ok(()),
        }
    }

    fn admit_values<'a>(&self, values: impl IntoIterator<Item = &'a Value>) -> Result<(), Error> {
        for value in values {
            self.admit_ordinary(value)?;
        }
        Ok(())
    }

    /// Rejects duplicate operand handles unless the counted heap actually
    /// contains at least that many owned references.
    fn ensure_owned<'a>(&self, values: impl IntoIterator<Item = &'a Value>) -> Result<(), Error> {
        let mut required = BTreeMap::new();
        for value in values {
            if let Value::Object(id) = value {
                *required.entry(id.id).or_insert(0) += 1;
            }
        }
        for (object, required) in required {
            let actual = self.object(object)?.references;
            if required > actual {
                return Err(Error::InsufficientReferences {
                    object,
                    required,
                    actual,
                });
            }
        }
        Ok(())
    }

    fn drop_values(&mut self, mut pending: Vec<Value>) -> Result<(), Error> {
        let mut failure = None;
        while let Some(value) = pending.pop() {
            if let Value::Object(id) = value {
                let object = match self.object_mut(id.id) {
                    Ok(object) => object,
                    Err(error) => {
                        failure.get_or_insert(error);
                        continue;
                    }
                };
                object.references -= 1;
                let dead = object.references == 0;
                self.stats.drops += 1;
                if dead {
                    let slot = &mut self.objects[id.id.index];
                    let object = slot.object.take().expect("the released object exists");
                    slot.generation = slot.generation.wrapping_add(1);
                    self.free_list.push(id.id.index);
                    self.stats.frees += 1;
                    self.stats.live_objects -= 1;
                    self.release_data(id.id, object.data, &mut pending);
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }

    fn release_data(&mut self, id: ObjectId, data: Data, pending: &mut Vec<Value>) {
        match data {
            Data::Continuation { execution, id } => {
                self.dropped_continuations.push((execution, id));
            }
            Data::ForeignHandle { execution, host_id }
            | Data::ForeignCycle { execution, host_id } => {
                self.released_foreign_handles.push((execution, host_id));
            }
            Data::Cell { contents, .. } => {
                self.stats.cells_freed += 1;
                pending.extend(contents);
            }
            Data::Arena { contents, .. } => {
                self.stats.arenas_freed += 1;
                pending.extend(contents);
            }
            Data::ServiceTag { key } => {
                if self.service_tags.get(&key) == Some(&id) {
                    self.service_tags.remove(&key);
                }
            }
            other => release_data_children(other, pending),
        }
    }
}

fn release_layer_data(layer: LayerData, pending: &mut Vec<Value>) {
    match layer {
        LayerData::Succeed {
            tag: first,
            implementation: second,
        }
        | LayerData::Merge {
            left: first,
            right: second,
        } => pending.extend([first, second]),
        LayerData::Context(v)
        | LayerData::Dynamic { build: v, .. }
        | LayerData::Fresh(v)
        | LayerData::OrDie(v) => pending.push(v),
        LayerData::MergeAll(layers) => pending.extend(layers),
        LayerData::Empty => {}
    }
}

fn release_lazy_state(state: Option<LazyState>, pending: &mut Vec<Value>) {
    match state {
        Some(LazyState::Pending(v) | LazyState::Running { task: v, .. } | LazyState::DoneOk(v)) => {
            pending.push(v);
        }
        Some(LazyState::DoneCause(c)) => pending.push(c.into_value()),
        None => {}
    }
}

fn release_data_children(data: Data, pending: &mut Vec<Value>) {
    match data {
        Data::RecoveryResult { selected, .. }
        | Data::Var {
            contents: selected, ..
        } => {
            pending.extend(selected);
        }
        Data::Cause(cause) => {
            pending.extend(
                cause
                    .reasons
                    .into_iter()
                    .filter_map(causes::StoredReason::into_value),
            );
            pending.extend(cause.trace);
        }
        Data::CauseReason(reason) => pending.extend(reason.into_value()),
        Data::Tagged { fields, .. }
        | Data::Closure {
            captures: fields, ..
        }
        | Data::Task {
            captures: fields, ..
        }
        | Data::Seq(SeqData::Generator {
            captures: fields, ..
        }) => pending.extend(fields),
        Data::ListSlice { backing: v, .. }
        | Data::Seq(SeqData::FromList(v))
        | Data::Exit(ExitData::Success(v) | ExitData::Failure(v))
        | Data::ExitTask(ExitData::Success(v) | ExitData::Failure(v))
        | Data::ContextRef { default_fn: v, .. }
        | Data::MaskedTask { task: v, .. } => pending.push(v),
        Data::Lazy { state, .. } => release_lazy_state(state, pending),
        Data::Iterator { state, .. } => {
            if let IteratorState::List { backing, .. } = state {
                pending.push(backing);
            }
        }
        Data::Dictionary(entries) => {
            pending.extend(entries.into_iter().map(|(_, entry)| entry.into_value()));
        }
        Data::Record(fields) => {
            pending.extend(fields.into_iter().map(|(_, value)| value));
        }
        Data::Layer(layer) => release_layer_data(layer, pending),
        Data::Context(entries) => pending.extend(entries.into_values()),
        Data::CapturedEnv { services, refs } => {
            pending.extend(services);
            pending.extend(refs.into_values());
        }
        Data::Graph(graph) => {
            pending.extend(graph.nodes.into_iter().map(|n| n.data));
            pending.extend(graph.edges.into_iter().map(|e| e.data));
        }
        Data::GraphWalker(walker) => {
            pending.extend(walker.indices);
            pending.extend(walker.values);
        }
        Data::ReuseToken { .. }
        | Data::Defect(_)
        | Data::Duration(_)
        | Data::DateTime(_)
        | Data::TimeZone(_)
        | Data::Cron(_)
        | Data::String(_)
        | Data::Utf16(_)
        | Data::Bytes(_)
        | Data::JsonNull
        | Data::BigDecimal { .. }
        | Data::RegExp { .. }
        | Data::BigInt(_)
        | Data::Label(_)
        | Data::FiberHandle { .. }
        | Data::Deferred { .. }
        | Data::MemoMap { .. }
        | Data::FiberGroup { .. }
        | Data::MaskRestore { .. }
        | Data::CachedRunTask { .. }
        | Data::CachedInvalidateTask { .. } => {}
        Data::Continuation { .. }
        | Data::ForeignHandle { .. }
        | Data::ForeignCycle { .. }
        | Data::Cell { .. }
        | Data::Arena { .. }
        | Data::ServiceTag { .. } => unreachable!("stateful heap data handled in release_data"),
    }
}
