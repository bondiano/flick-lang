impl Heap {
    pub fn new() -> Self {
        let identity = NEXT_HEAP
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("fewer than usize::MAX model heaps");
        Self {
            identity,
            objects: Vec::new(),
            stats: Stats::default(),
            service_tags: BTreeMap::new(),
        }
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
        let actual = self.objects.iter().filter(|slot| slot.is_some()).count();
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
            } => fields.get(field).ok_or(Error::MissingField {
                object: id.id,
                field,
            }),
            Data::RecoveryResult { .. }
            | Data::Cause(_)
            | Data::Defect(_)
            | Data::ServiceTag { .. }
            | Data::Layer(_)
            | Data::String(_)
            | Data::BigInt(_)
            | Data::Record(_)
            | Data::Dictionary(_)
            | Data::Cell { .. }
            | Data::Arena { .. } => Err(Error::ExpectedObject),
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
            Data::RecoveryResult { .. }
            | Data::Cause(_)
            | Data::Defect(_)
            | Data::ServiceTag { .. }
            | Data::Layer(_)
            | Data::String(_)
            | Data::BigInt(_)
            | Data::Record(_)
            | Data::Dictionary(_)
            | Data::Closure { .. }
            | Data::Task { .. }
            | Data::Cell { .. }
            | Data::Arena { .. } => Err(Error::ExpectedObject),
        }
    }

    pub fn string(&self, value: &Value) -> Result<&str, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::ExpectedString);
        };
        match &self.object(id.id)?.data {
            Data::String(value) => Ok(value),
            Data::RecoveryResult { .. }
            | Data::Cause(_)
            | Data::Defect(_)
            | Data::ServiceTag { .. }
            | Data::Layer(_)
            | Data::Tagged { .. }
            | Data::BigInt(_)
            | Data::Record(_)
            | Data::Dictionary(_)
            | Data::Closure { .. }
            | Data::Task { .. }
            | Data::Cell { .. }
            | Data::Arena { .. } => Err(Error::ExpectedString),
        }
    }

    /// The exact unbounded literal spelling; arithmetic is outside this
    /// memory interpreter, but its allocated payload is reference-counted.
    pub fn bigint(&self, value: &Value) -> Result<&str, Error> {
        self.admit_ordinary(value)?;
        let Value::Object(id) = value else {
            return Err(Error::ExpectedBigInt);
        };
        match &self.object(id.id)?.data {
            Data::BigInt(value) => Ok(value),
            Data::RecoveryResult { .. }
            | Data::Cause(_)
            | Data::Defect(_)
            | Data::ServiceTag { .. }
            | Data::Layer(_)
            | Data::Tagged { .. }
            | Data::String(_)
            | Data::Record(_)
            | Data::Dictionary(_)
            | Data::Closure { .. }
            | Data::Task { .. }
            | Data::Cell { .. }
            | Data::Arena { .. } => Err(Error::ExpectedBigInt),
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
