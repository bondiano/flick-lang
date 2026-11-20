impl Heap {
    pub fn new() -> Self {
        let identity = NEXT_HEAP
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("fewer than usize::MAX model heaps");
        Self {
            identity,
            objects: Vec::new(),
            free_list: Vec::new(),
            stats: Stats::default(),
            service_tags: BTreeMap::new(),
            dropped_continuations: Vec::new(),
        }
    }

    /// Drains continuation IDs whose last counted reference hit zero in `execution`.
    pub(super) fn take_dropped_continuations(&mut self, execution: usize) -> Vec<usize> {
        let mut taken = Vec::new();
        let mut kept = Vec::new();
        for (owner, id) in mem::take(&mut self.dropped_continuations) {
            if owner == execution {
                taken.push(id);
            } else {
                kept.push((owner, id));
            }
        }
        self.dropped_continuations = kept;
        taken
    }

    /// Resolves either a flat `$list` or an O(1) `ListSlice` into `(backing_id, offset, len)`.
    pub(super) fn list_view(&self, value: &Value) -> Result<(ObjectId, usize, usize), Error> {
        self.admit_ordinary(value)?;
        let Value::Object(token) = value else {
            return Err(Error::PatternKind("list"));
        };
        match &self.object(token.id)?.data {
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
            } if tag == "$list" => {
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                Ok((token.id, 0, fields.len()))
            }
            Data::ListSlice {
                backing,
                offset,
                len,
            } => {
                let Value::Object(backing_token) = backing else {
                    return Err(Error::PatternKind("list"));
                };
                let Data::Tagged {
                    tag,
                    ordinary_fields,
                    fields,
                } = &self.object(backing_token.id)?.data
                else {
                    return Err(Error::PatternKind("list"));
                };
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                if tag != "$list" || offset.saturating_add(*len) > fields.len() {
                    return Err(Error::PatternKind("list"));
                }
                Ok((backing_token.id, *offset, *len))
            }
            _ => Err(Error::PatternKind("list")),
        }
    }

    pub(super) fn label_str(&self, value: &Value) -> Result<&str, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(token) = value else {
            return Err(Error::ExpectedLabel);
        };
        match &self.object(token.id)?.data {
            Data::Label(name) | Data::String(name) => Ok(name),
            _ => Err(Error::ExpectedLabel),
        }
    }

    pub(super) fn var_contents<'a>(
        &'a self,
        execution: usize,
        var: &VarHandle,
    ) -> Result<&'a Value, Error> {
        if var.execution != execution {
            return Err(Error::ForeignVar);
        }
        let slot = self
            .objects
            .get(var.id.index)
            .filter(|_| var.id.heap == self.identity)
            .ok_or(Error::ForeignVar)?;
        let object = slot
            .object
            .as_ref()
            .filter(|_| slot.generation == var.id.generation)
            .ok_or(Error::ClosedVar)?;
        let Data::Var {
            execution: owner,
            contents,
        } = &object.data
        else {
            return Err(Error::ExpectedVar);
        };
        if *owner != execution {
            return Err(Error::ForeignVar);
        }
        contents.as_ref().ok_or(Error::ClosedVar)
    }

    pub(super) fn var_contents_mut<'a>(
        &'a mut self,
        execution: usize,
        var: &VarHandle,
    ) -> Result<&'a mut Value, Error> {
        if var.execution != execution {
            return Err(Error::ForeignVar);
        }
        let slot = self
            .objects
            .get_mut(var.id.index)
            .filter(|_| var.id.heap == self.identity)
            .ok_or(Error::ForeignVar)?;
        if slot.generation != var.id.generation {
            return Err(Error::ClosedVar);
        }
        let object = slot.object.as_mut().ok_or(Error::ClosedVar)?;
        let Data::Var {
            execution: owner,
            contents,
        } = &mut object.data
        else {
            return Err(Error::ExpectedVar);
        };
        if *owner != execution {
            return Err(Error::ForeignVar);
        }
        contents.as_mut().ok_or(Error::ClosedVar)
    }

    fn lazy_state_mut<'a>(
        &'a mut self,
        execution: usize,
        lazy: &LazyHandle,
    ) -> Result<&'a mut Option<LazyState>, Error> {
        if lazy.execution != execution {
            return Err(Error::ForeignLazy);
        }
        let slot = self
            .objects
            .get_mut(lazy.id.index)
            .filter(|_| lazy.id.heap == self.identity)
            .ok_or(Error::ForeignLazy)?;
        if slot.generation != lazy.id.generation {
            return Err(Error::ClosedLazy);
        }
        let object = slot.object.as_mut().ok_or(Error::ClosedLazy)?;
        let Data::Lazy {
            execution: owner,
            state,
        } = &mut object.data
        else {
            return Err(Error::ExpectedLazy);
        };
        if *owner != execution {
            return Err(Error::ForeignLazy);
        }
        Ok(state)
    }

    fn iterator_state_mut<'a>(
        &'a mut self,
        execution: usize,
        iterator: &IteratorHandle,
    ) -> Result<&'a mut IteratorState, Error> {
        if iterator.execution != execution {
            return Err(Error::ForeignIterator);
        }
        let slot = self
            .objects
            .get_mut(iterator.id.index)
            .filter(|_| iterator.id.heap == self.identity)
            .ok_or(Error::ForeignIterator)?;
        if slot.generation != iterator.id.generation {
            return Err(Error::ClosedIterator);
        }
        let object = slot.object.as_mut().ok_or(Error::ClosedIterator)?;
        let Data::Iterator {
            execution: owner,
            state,
        } = &mut object.data
        else {
            return Err(Error::ExpectedIterator);
        };
        if *owner != execution {
            return Err(Error::ForeignIterator);
        }
        Ok(state)
    }

    /// Canonicalizes simultaneous live tags within this heap, without a strong cache.
    pub fn service_tag(&mut self, key: &str) -> Result<Value, Error> {
        if let Some(id) = self.service_tags.get(key).copied() {
            let value = Value::Object(ObjectToken { id });
            return self.dup(&value);
        }
        let value = self.allocate(Data::ServiceTag { key: key.into() });
        self.service_tags
            .insert(key.into(), value.object_id().expect("allocated tag"));
        Ok(value)
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Tests a diagnostic identity without creating an owned reference.
    pub fn is_live(&self, id: ObjectId) -> bool {
        self.object(id).is_ok()
    }

    /// A check, not a collector. The caller must explicitly drop its
    /// returned value and all other roots before this can succeed.
    pub fn finish(&self) -> Result<Stats, Error> {
        let actual = self
            .objects
            .iter()
            .filter(|slot| slot.object.is_some())
            .count();
        if actual != self.stats.live_objects {
            return Err(Error::LiveCountMismatch {
                tracked: self.stats.live_objects,
                actual,
            });
        }
        if actual == 0 {
            Ok(self.stats)
        } else {
            Err(Error::LiveObjects(actual))
        }
    }

    pub fn literal(&mut self, literal: &Lit) -> Result<Value, Error> {
        Ok(match literal {
            Lit::Unit => Value::Unit,
            Lit::Bool(value) => Value::Bool(*value),
            Lit::Int(value) => Value::Int(*value),
            Lit::Float(value) => Value::Float(*value),
            Lit::String(value) => self.allocate(Data::String(value.clone())),
            Lit::BigInt(value) => self.allocate(Data::BigInt(value.clone())),
        })
    }

    /// Transfers ownership of the fields into a new object.
    pub fn construct(&mut self, tag: &str, fields: Vec<Value>) -> Result<Value, Error> {
        self.construct_with_dicts(tag, fields, vec![])
    }

    /// Transfers both vectors into physical prefix/suffix storage. Evidence
    /// remains strongly owned but is not part of structural value equality.
    /// Accepted owned operands are released on a layout-validation error.
    pub fn construct_with_dicts(
        &mut self,
        tag: &str,
        mut fields: Vec<Value>,
        dicts: Vec<Value>,
    ) -> Result<Value, Error> {
        self.ensure_owned(fields.iter().chain(&dicts))?;
        if let Err(error) = self.admit_values(fields.iter().chain(&dicts)) {
            fields.extend(dicts);
            self.drop_values(fields)?;
            return Err(error);
        }
        let ordinary_fields = fields.len();
        let total = ordinary_fields
            .checked_add(dicts.len())
            .ok_or(Error::ConstructorFieldOverflow)?;
        if let Err(error) = tagged_boundary(tag, ordinary_fields, total) {
            fields.extend(dicts);
            self.drop_values(fields)?;
            return Err(error);
        }
        fields.extend(dicts);
        Ok(self.allocate(Data::Tagged {
            tag: tag.to_owned(),
            ordinary_fields,
            fields,
        }))
    }

    /// Returns a borrowed field. An independently owned field token is
    /// obtained with [`Self::project`], not by copying its identity.
    pub fn field<'a>(&'a self, value: &Value, field: usize) -> Result<&'a Value, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::ExpectedObject);
        };
        match &self.object(id.id)?.data {
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
            } => {
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                fields.get(field).ok_or(Error::MissingField {
                    object: id.id,
                    field,
                })
            }
            Data::Closure {
                captures: fields, ..
            }
            | Data::Task {
                captures: fields, ..
            }
            | Data::Seq(SeqData::Generator {
                captures: fields, ..
            }) => fields.get(field).ok_or(Error::MissingField {
                object: id.id,
                field,
            }),
            Data::ListSlice {
                backing,
                offset,
                len,
            } => {
                if field >= *len {
                    return Err(Error::MissingField {
                        object: id.id,
                        field,
                    });
                }
                let Value::Object(backing_token) = backing else {
                    return Err(Error::ExpectedObject);
                };
                let Data::Tagged { fields, .. } = &self.object(backing_token.id)?.data else {
                    return Err(Error::ExpectedObject);
                };
                fields.get(offset + field).ok_or(Error::MissingField {
                    object: id.id,
                    field,
                })
            }
            Data::RecoveryResult { .. }
            | Data::Cause(_)
            | Data::CauseReason(_)
            | Data::Duration(_)
            | Data::Defect(_)
            | Data::ServiceTag { .. }
            | Data::Layer(_)
            | Data::String(_)
            | Data::Utf16(_)
            | Data::Bytes(_)
            | Data::JsonNull
            | Data::BigDecimal { .. }
            | Data::RegExp { .. }
            | Data::BigInt(_)
            | Data::Record(_)
            | Data::Dictionary(_)
            | Data::Continuation { .. }
            | Data::Cell { .. }
            | Data::Var { .. }
            | Data::Lazy { .. }
            | Data::Arena { .. }
            | Data::Seq(_)
            | Data::Iterator { .. }
            | Data::Label(_)
            | Data::Exit(_)
            | Data::DateTime(_)
            | Data::TimeZone(_)
            | Data::Cron(_)
            | Data::Graph(_)
            | Data::GraphWalker(_) => Err(Error::ExpectedObject),
        }
    }

    pub fn tag(&self, value: &Value) -> Result<&str, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::ExpectedObject);
        };
        match &self.object(id.id)?.data {
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
            } => {
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                Ok(tag)
            }
            Data::ListSlice { .. } => Ok("$list"),
            Data::RecoveryResult { .. }
            | Data::Cause(_)
            | Data::CauseReason(_)
            | Data::Duration(_)
            | Data::Defect(_)
            | Data::ServiceTag { .. }
            | Data::Layer(_)
            | Data::String(_)
            | Data::Utf16(_)
            | Data::Bytes(_)
            | Data::JsonNull
            | Data::BigDecimal { .. }
            | Data::RegExp { .. }
            | Data::BigInt(_)
            | Data::Record(_)
            | Data::Dictionary(_)
            | Data::Closure { .. }
            | Data::Task { .. }
            | Data::Continuation { .. }
            | Data::Cell { .. }
            | Data::Var { .. }
            | Data::Lazy { .. }
            | Data::Arena { .. }
            | Data::Seq(_)
            | Data::Iterator { .. }
            | Data::Label(_)
            | Data::Exit(_)
            | Data::DateTime(_)
            | Data::TimeZone(_)
            | Data::Cron(_)
            | Data::Graph(_)
            | Data::GraphWalker(_) => Err(Error::ExpectedObject),
        }
    }

    pub fn string(&self, value: &Value) -> Result<&str, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::ExpectedString);
        };
        match &self.object(id.id)?.data {
            Data::String(value) => Ok(value),
            _ => Err(Error::ExpectedString),
        }
    }

    pub(super) fn utf16_units(&self, value: &Value) -> Result<Vec<u16>, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::ExpectedString);
        };
        match &self.object(id.id)?.data {
            Data::String(s) => Ok(s.encode_utf16().collect()),
            Data::Utf16(units) => Ok(units.clone()),
            _ => Err(Error::ExpectedString),
        }
    }

    pub(super) fn alloc_utf16(&mut self, units: Vec<u16>) -> Value {
        match String::from_utf16(&units) {
            Ok(s) => self.allocate(Data::String(s)),
            Err(_) => self.allocate(Data::Utf16(units)),
        }
    }

    pub(super) fn string_lossy(&self, value: &Value) -> Result<String, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::ExpectedString);
        };
        match &self.object(id.id)?.data {
            Data::String(s) => Ok(s.clone()),
            Data::Utf16(units) => Ok(String::from_utf16_lossy(units)),
            _ => Err(Error::ExpectedString),
        }
    }

    /// The exact unbounded literal spelling; arithmetic is outside this
    /// memory interpreter, but its allocated payload is reference-counted.
    pub(super) fn bytes(&self, value: &Value) -> Result<&[u8], Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::PatternKind("bytes"));
        };
        match &self.object(id.id)?.data {
            Data::Bytes(buf) => Ok(buf),
            _ => Err(Error::PatternKind("bytes")),
        }
    }

    pub fn bigint(&self, value: &Value) -> Result<&str, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::ExpectedBigInt);
        };
        match &self.object(id.id)?.data {
            Data::BigInt(value) => Ok(value),
            _ => Err(Error::ExpectedBigInt),
        }
    }

    pub fn reference_count(&self, value: &Value) -> Result<usize, Error> {
        match value {
            Value::Object(id) => Ok(self.object(id.id)?.references),
            _ => Ok(0),
        }
    }

    /// Borrows the receiver; the result owns an additional field token.
    pub fn project(&mut self, value: &Value, field: usize) -> Result<Value, Error> {
        let field = self.field(value, field)?.representation();
        self.admit_ordinary(&field)?;
        self.dup(&field)
    }

    pub fn record(&mut self, fields: Vec<(String, Value)>) -> Result<Value, Error> {
        let mut labels = BTreeSet::new();
        for (name, _) in &fields {
            if !labels.insert(name) {
                return Err(Error::DuplicateField(name.clone()));
            }
        }
        self.ensure_owned(fields.iter().map(|(_, value)| value))?;
        if let Err(error) = self.admit_values(fields.iter().map(|(_, value)| value)) {
            self.drop_values(fields.into_iter().map(|(_, value)| value).collect())?;
            return Err(error);
        }
        Ok(self.allocate(Data::Record(fields)))
    }

    /// Borrows an actual record field. Constructor metadata is materialized
    /// as an owned string only by [`Self::project_named`].
    pub fn named_field<'a>(&'a self, value: &Value, field: &str) -> Result<&'a Value, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(token) = value else {
            return Err(Error::ExpectedRecord);
        };
        let Data::Record(fields) = &self.object(token.id)?.data else {
            return Err(Error::ExpectedRecord);
        };
        fields
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, value)| value)
            .ok_or_else(|| Error::MissingNamedField(field.into()))
    }

    /// Borrows its receiver and returns an owned record field or the full
    /// nominal tag of a constructor. A record's own `_tag` is ordinary data.
    pub fn project_named(&mut self, value: &Value, field: &str) -> Result<Value, Error> {
        self.admit_ordinary(value)?;
        if field == "_tag"
            && let Value::Object(token) = value
            && let Data::Tagged {
                tag,
                ordinary_fields,
                fields,
            } = &self.object(token.id)?.data
        {
            tagged_boundary(tag, *ordinary_fields, fields.len())?;
            let tag = tag.clone();
            return self.literal(&Lit::String(tag));
        }
        let field = self.named_field(value, field)?.representation();
        self.admit_ordinary(&field)?;
        self.dup(&field)
    }
}
