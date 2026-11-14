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

    /// Reuses only a unique tagged object of exactly the same field count.
    /// Alias/layout/string/immediate cases use ordinary drop + allocation.
    pub fn reuse(
        &mut self,
        candidate: Value,
        tag: &str,
        fields: Vec<Value>,
    ) -> Result<Value, Error> {
        self.reuse_with_dicts(candidate, tag, fields, vec![])
    }

    /// Reuses physical capacity, not the old semantic partition. Both
    /// ordinary/evidence metadata and every owned field are replaced.
    pub fn reuse_with_dicts(
        &mut self,
        candidate: Value,
        tag: &str,
        mut fields: Vec<Value>,
        dicts: Vec<Value>,
    ) -> Result<Value, Error> {
        self.ensure_owned(iter::once(&candidate).chain(&fields).chain(&dicts))?;
        if let Err(error) = self.admit_values(iter::once(&candidate).chain(&fields).chain(&dicts)) {
            fields.extend(dicts);
            fields.push(candidate);
            self.drop_values(fields)?;
            return Err(error);
        }
        let ordinary_fields = fields.len();
        let total = ordinary_fields
            .checked_add(dicts.len())
            .ok_or(Error::ConstructorFieldOverflow)?;
        if let Err(error) = tagged_boundary(tag, ordinary_fields, total) {
            fields.extend(dicts);
            fields.push(candidate);
            self.drop_values(fields)?;
            return Err(error);
        }
        fields.extend(dicts);
        let reusable = match self.can_reuse(&candidate, total) {
            Ok(reusable) => reusable,
            Err(error) => {
                fields.push(candidate);
                self.drop_values(fields)?;
                return Err(error);
            }
        };
        if reusable {
            let Value::Object(id) = candidate else {
                unreachable!("only objects are reusable")
            };
            let old = mem::replace(
                &mut self.object_mut(id.id)?.data,
                Data::Tagged {
                    tag: tag.to_owned(),
                    ordinary_fields,
                    fields,
                },
            );
            self.stats.reuses += 1;
            if let Data::Tagged { fields, .. } = old {
                self.drop_values(fields)?;
            }
            Ok(Value::Object(id))
        } else {
            self.drop_value(candidate)?;
            Ok(self.allocate(Data::Tagged {
                tag: tag.to_owned(),
                ordinary_fields,
                fields,
            }))
        }
    }

    fn can_reuse(&self, candidate: &Value, total: usize) -> Result<bool, Error> {
        let Value::Object(id) = candidate else {
            return Ok(false);
        };
        let object = self.object(id.id)?;
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } = &object.data
        else {
            return Ok(false);
        };
        tagged_boundary(tag, *ordinary_fields, fields.len())?;
        Ok(object.references == 1 && fields.len() == total)
    }

    fn allocate(&mut self, data: Data) -> Value {
        self.stats.cells_allocated += usize::from(matches!(data, Data::Cell { .. }));
        self.stats.arenas_allocated += usize::from(matches!(data, Data::Arena { .. }));
        let id = ObjectId {
            heap: self.identity,
            index: self.objects.len(),
        };
        self.objects.push(Some(Object {
            references: 1,
            data,
        }));
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
            .and_then(Option::as_ref)
            .ok_or(Error::InvalidObject(id))
    }

    fn object_mut(&mut self, id: ObjectId) -> Result<&mut Object, Error> {
        if id.heap != self.identity {
            return Err(Error::InvalidObject(id));
        }
        self.objects
            .get_mut(id.index)
            .and_then(Option::as_mut)
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
        // VM-minted private ids never recycle: only an in-range retired slot is closed.
        let object = self
            .objects
            .get(arena.id.index)
            .ok_or(Error::InvalidObject(arena.id))?
            .as_ref()
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
        let object = self
            .objects
            .get_mut(arena.id.index)
            .ok_or(Error::InvalidObject(arena.id))?
            .as_mut()
            .ok_or(Error::ClosedArena)?;
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
        if let Value::Object(token) = value
            && matches!(self.object(token.id)?.data, Data::RecoveryResult { .. })
        {
            return Err(Error::RecoveryEscape);
        }
        Ok(())
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
                    let object = self.objects[id.id.index]
                        .take()
                        .expect("the released object exists");
                    self.stats.frees += 1;
                    self.stats.live_objects -= 1;
                    match object.data {
                        Data::RecoveryResult { selected, .. } => pending.extend(selected),
                        Data::Cause(reasons) => pending.extend(
                            reasons
                                .into_iter()
                                .filter_map(causes::StoredReason::into_value),
                        ),
                        Data::Tagged { fields, .. }
                        | Data::Closure {
                            captures: fields, ..
                        }
                        | Data::Task {
                            captures: fields, ..
                        } => pending.extend(fields),
                        Data::Cell { contents, .. } => {
                            self.stats.cells_freed += 1;
                            pending.extend(contents);
                        }
                        Data::Arena { contents, .. } => {
                            self.stats.arenas_freed += 1;
                            pending.extend(contents);
                        }
                        Data::Dictionary(entries) => {
                            pending
                                .extend(entries.into_iter().map(|(_, entry)| entry.into_value()));
                        }
                        Data::Record(fields) => {
                            pending.extend(fields.into_iter().map(|(_, value)| value));
                        }
                        Data::ServiceTag { key } => {
                            if self.service_tags.get(&key) == Some(&id.id) {
                                self.service_tags.remove(&key);
                            }
                        }
                        Data::Layer(LayerData::Succeed {
                            tag,
                            implementation,
                        }) => {
                            pending.extend([tag, implementation]);
                        }
                        Data::Layer(LayerData::Merge { left, right }) => {
                            pending.extend([left, right]);
                        }
                        Data::Defect(_) | Data::String(_) | Data::BigInt(_) => {}
                    }
                }
            }
        }
        failure.map_or(Ok(()), Err)
    }
}
