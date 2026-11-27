// Module attribution, live-heap inventory, and snapshot/restore support on `Heap`.

impl Heap {
    /// Sets the active module tag applied to subsequent allocations, returning
    /// the previous tag for restoration.
    pub(crate) fn set_allocation_module(&mut self, module: Option<&str>) -> Option<u32> {
        let next = module.map(|name| {
            if let Some(&idx) = self.module_indices.get(name) {
                idx
            } else {
                let idx = u32::try_from(self.module_names.len())
                    .expect("fewer than u32::MAX modules in one heap");
                self.module_names.push(name.to_owned());
                self.module_indices.insert(name.to_owned(), idx);
                idx
            }
        });
        mem::replace(&mut self.active_module, next)
    }

    /// Restores a previously saved module attribution tag.
    pub(crate) fn restore_allocation_module(&mut self, previous: Option<u32>) {
        self.active_module = previous;
    }

    /// Returns the module name that allocated `id`, if attributed.
    pub(crate) fn object_owner_module(&self, id: ObjectId) -> Option<&str> {
        let obj = self.object(id).ok()?;
        let idx = obj.owner_module?;
        self.module_names.get(idx as usize).map(String::as_str)
    }

    /// Constructs a non-owning [`Value::Cell`] handle for an allocated `Data::Cell`.
    pub(crate) fn cell_handle(id: ObjectId, execution: usize) -> Value {
        Value::Cell(CellHandle { id, execution })
    }

    /// Constructs a non-owning [`Value::Lazy`] handle for an allocated `Data::Lazy`.
    pub(crate) fn lazy_handle(id: ObjectId, execution: usize) -> Value {
        Value::Lazy(LazyHandle { id, execution })
    }

    /// Extracts the underlying [`ObjectId`] of a [`Value::Lazy`] handle.
    pub(crate) fn lazy_id(value: &Value) -> Option<ObjectId> {
        match value {
            Value::Lazy(lazy) => Some(lazy.id),
            _ => None,
        }
    }

    /// Counts live heap slots attributed to each module.
    ///
    /// Closures, tasks, and lazy generators are attributed to the owner of
    /// their `CodeId` when known in `code_owner`, falling back to the
    /// allocation-time module tag.
    pub(crate) fn module_slot_counts(
        &self,
        code_owner: &BTreeMap<memory::CodeId, String>,
    ) -> (BTreeMap<String, usize>, usize, usize) {
        let mut by_module: BTreeMap<String, usize> = BTreeMap::new();
        let mut unattributed = 0usize;
        let mut total = 0usize;
        for slot in &self.objects {
            let Some(obj) = &slot.object else {
                continue;
            };
            total += 1;
            let code_mod = match &obj.data {
                Data::Closure { code, .. }
                | Data::Task { code, .. }
                | Data::Seq(SeqData::Generator { code, .. }) => code_owner.get(code).map(String::as_str),
                _ => None,
            };
            let owner = code_mod.or_else(|| {
                obj.owner_module
                    .and_then(|idx| self.module_names.get(idx as usize).map(String::as_str))
            });
            if let Some(module) = owner {
                *by_module.entry(module.to_owned()).or_insert(0) += 1;
            } else {
                unattributed += 1;
            }
        }
        (by_module, unattributed, total)
    }

    /// Collects all `CodeId`s currently referenced by live closures, tasks, or
    /// lazy generators on this heap.
    pub(crate) fn live_code_ids(&self) -> BTreeSet<memory::CodeId> {
        let mut codes = BTreeSet::new();
        for slot in &self.objects {
            let Some(obj) = &slot.object else {
                continue;
            };
            match &obj.data {
                Data::Closure { code, .. }
                | Data::Task { code, .. }
                | Data::Seq(SeqData::Generator { code, .. }) => {
                    codes.insert(*code);
                }
                _ => {}
            }
        }
        codes
    }

    /// Collects all `Cell` and `Lazy` [`ObjectId`]s reachable from `roots`.
    /// Used when unloading modules to keep root-arena cells/lazies alive while
    /// any live closure or root in another module still references them.
    pub(crate) fn reachable_cell_and_lazy_ids<'a>(
        &self,
        roots: impl IntoIterator<Item = &'a Value>,
    ) -> BTreeSet<ObjectId> {
        let mut visited: BTreeSet<ObjectId> = BTreeSet::new();
        let mut handles: BTreeSet<ObjectId> = BTreeSet::new();
        let mut stack: Vec<&Value> = roots.into_iter().collect();
        while let Some(value) = stack.pop() {
            match value {
                Value::Object(token) => {
                    if visited.insert(token.id)
                        && let Ok(obj) = self.object(token.id)
                    {
                        push_data_child_refs(&obj.data, &mut stack);
                    }
                }
                Value::Cell(cell) => {
                    handles.insert(cell.id);
                    if visited.insert(cell.id)
                        && let Ok(obj) = self.object(cell.id)
                    {
                        push_data_child_refs(&obj.data, &mut stack);
                    }
                }
                Value::Lazy(lazy) => {
                    handles.insert(lazy.id);
                    if visited.insert(lazy.id)
                        && let Ok(obj) = self.object(lazy.id)
                    {
                        push_data_child_refs(&obj.data, &mut stack);
                    }
                }
                Value::Aggregate { fields, .. } => stack.extend(fields.iter()),
                _ => {}
            }
        }
        handles
    }
}

fn push_data_child_refs<'a>(data: &'a Data, out: &mut Vec<&'a Value>) {
    match data {
        Data::Tagged { fields, .. }
        | Data::Closure {
            captures: fields, ..
        }
        | Data::Task {
            captures: fields, ..
        }
        | Data::Seq(SeqData::Generator {
            captures: fields, ..
        })
        | Data::Arena {
            contents: fields, ..
        } => out.extend(fields.iter()),
        Data::ListSlice { backing: v, .. }
        | Data::Seq(SeqData::FromList(v))
        | Data::Exit(ExitData::Success(v) | ExitData::Failure(v))
        | Data::ExitTask(ExitData::Success(v) | ExitData::Failure(v))
        | Data::ContextRef { default_fn: v, .. }
        | Data::MaskedTask { task: v, .. } => out.push(v),
        Data::BoxedAggregate { value, .. } => out.push(value),
        Data::Record(fields) => out.extend(fields.iter().map(|(_, v)| v)),
        Data::Dictionary(entries) => out.extend(entries.iter().map(|(_, v)| v.value())),
        Data::Cell { contents, .. }
        | Data::Var { contents, .. }
        | Data::RecoveryResult {
            selected: contents, ..
        } => out.extend(contents.iter()),
        Data::Lazy { state, .. } => match state {
            Some(
                LazyState::Pending(v)
                | LazyState::Running { task: v, .. }
                | LazyState::RunningLocal { task: Some(v) }
                | LazyState::DoneOk(v),
            ) => out.push(v),
            Some(LazyState::DoneCause(c)) => {
                // `CauseToken` stores an owned object; traverse its cause slot via cell-like lookup?
                // Handled by CauseToken's object id below if needed, or push via representation:
                let _ = c;
            }
            Some(LazyState::RunningLocal { task: None }) | None => {}
        },
        Data::Layer(layer) => match layer {
            LayerData::Succeed {
                tag,
                implementation,
            } => out.extend([tag, implementation]),
            LayerData::Merge { left, right } => out.extend([left, right]),
            LayerData::Context(v)
            | LayerData::Dynamic { build: v, .. }
            | LayerData::Fresh(v)
            | LayerData::OrDie(v) => out.push(v),
            LayerData::MergeAll(layers) => out.extend(layers.iter()),
            LayerData::Empty => {}
        },
        Data::Context(entries) => out.extend(entries.values()),
        Data::CapturedEnv { services, refs } => {
            out.extend(services.iter());
            out.extend(refs.values());
        }
        Data::Graph(graph) => {
            out.extend(graph.nodes.iter().map(|n| &n.data));
            out.extend(graph.edges.iter().map(|e| &e.data));
        }
        Data::GraphWalker(walker) => {
            out.extend(walker.indices.iter());
            out.extend(walker.values.iter());
        }
        Data::Cause(cause) => {
            for reason in &cause.reasons {
                if let causes::StoredReason::Fail(v) | causes::StoredReason::Die(v) = reason {
                    out.push(v);
                }
            }
            out.extend(cause.trace.iter());
        }
        Data::CauseReason(causes::StoredReason::Fail(v) | causes::StoredReason::Die(v)) => {
            out.push(v);
        }
        Data::Iterator {
            state: IteratorState::List { backing, .. },
            ..
        } => out.push(backing),
        _ => {}
    }
}
