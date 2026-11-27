impl Heap {
    /// Detaches a frame-limited reuse token from a uniquely owned `Data::Tagged`
    /// or `Data::Record`, immediately dropping all old children so child
    /// uniqueness and observable cleanup order are preserved. Shared or
    /// non-reusable values are dropped normally and return `Value::ReuseNull`.
    pub fn drop_reuse(&mut self, candidate: Value) -> Result<Value, Error> {
        self.ensure_owned(iter::once(&candidate))?;
        if let Err(error) = self.admit_ordinary(&candidate) {
            self.drop_value(candidate)?;
            return Err(error);
        }
        if matches!(candidate, Value::Aggregate { .. }) {
            self.drop_value(candidate)?;
            return Err(Error::AggregateReuseCandidate);
        }
        if !self.reuse_enabled {
            self.drop_value(candidate)?;
            return Ok(Value::ReuseNull);
        }
        let Value::Object(token) = candidate else {
            return Ok(Value::ReuseNull);
        };
        let detach = {
            let object = self.object(token.id)?;
            if object.references == 1 {
                match &object.data {
                    Data::Tagged {
                        tag,
                        ordinary_fields,
                        fields,
                        layout,
                    } => {
                        if let Err(error) = tagged_boundary(tag, *ordinary_fields, fields.len()) {
                            self.drop_value(Value::Object(token))?;
                            return Err(error);
                        }
                        Some((ReuseKind::Tagged, fields.len(), layout.clone()))
                    }
                    Data::Record(fields) => Some((ReuseKind::Record, fields.len(), None)),
                    _ => None,
                }
            } else {
                None
            }
        };
        let Some((kind, capacity, layout)) = detach else {
            self.drop_value(Value::Object(token))?;
            return Ok(Value::ReuseNull);
        };
        let old = self.replace_object_data(
            token.id,
            Data::ReuseToken {
                kind,
                capacity,
                layout,
            },
        )?;
        match old {
            Data::Tagged { fields, .. } => self.drop_values(fields)?,
            Data::Record(fields) => {
                self.drop_values(fields.into_iter().map(|(_, value)| value).collect())?;
            }
            _ => unreachable!("verified reusable tagged or record slot"),
        }
        Ok(Value::Object(token))
    }

    /// Reuses only a unique tagged object or tagged reuse token of exactly the
    /// same field count. Alias/layout/string/immediate cases use ordinary drop + allocation.
    pub fn reuse(
        &mut self,
        candidate: Value,
        tag: &str,
        fields: Vec<Value>,
    ) -> Result<Value, Error> {
        self.reuse_with_dicts_and_layout(candidate, tag, fields, vec![], None)
    }

    /// Reuses a unique tagged object or reuse token when field count AND
    /// abstract `CellLayout` are compatible; otherwise drops and allocates.
    pub fn reuse_with_layout(
        &mut self,
        candidate: Value,
        tag: &str,
        fields: Vec<Value>,
        layout: Option<CellLayout>,
    ) -> Result<Value, Error> {
        self.reuse_with_dicts_and_layout(candidate, tag, fields, vec![], layout)
    }

    /// Reuses physical capacity, not the old semantic partition. Both
    /// ordinary/evidence metadata and every owned field are replaced.
    pub fn reuse_with_dicts(
        &mut self,
        candidate: Value,
        tag: &str,
        fields: Vec<Value>,
        dicts: Vec<Value>,
    ) -> Result<Value, Error> {
        self.reuse_with_dicts_and_layout(candidate, tag, fields, dicts, None)
    }

    /// Reuses physical capacity when unique, equal-sized, and layout-compatible.
    /// Rejects `Value::Aggregate` candidates with [`Error::AggregateReuseCandidate`].
    pub fn reuse_with_dicts_and_layout(
        &mut self,
        candidate: Value,
        tag: &str,
        mut fields: Vec<Value>,
        dicts: Vec<Value>,
        layout: Option<CellLayout>,
    ) -> Result<Value, Error> {
        self.ensure_owned(iter::once(&candidate).chain(&fields).chain(&dicts))?;
        if let Err(error) = self
            .admit_reuse_candidate(&candidate)
            .and_then(|()| self.admit_values(fields.iter().chain(&dicts)))
        {
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
        let reusable = match self.can_reuse(&candidate, total, layout.as_ref()) {
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
            let old = self.replace_object_data(
                id.id,
                Data::Tagged {
                    tag: tag.to_owned(),
                    ordinary_fields,
                    fields,
                    layout,
                },
            )?;
            self.stats.reuses += 1;
            if let Data::Tagged { fields, .. } = old {
                self.drop_values(fields)?;
            }
            Ok(Value::Object(id))
        } else {
            self.stats.reuse_fallbacks += 1;
            self.drop_value(candidate)?;
            self.try_allocate(Data::Tagged {
                tag: tag.to_owned(),
                ordinary_fields,
                fields,
                layout,
            })
        }
    }

    /// Reuses a unique record or record reuse token with the exact same field
    /// count; otherwise drops `candidate` and allocates a fresh record.
    pub fn reuse_record(
        &mut self,
        candidate: Value,
        fields: Vec<(String, Value)>,
    ) -> Result<Value, Error> {
        let mut labels = BTreeSet::new();
        let mut duplicate = None;
        for (name, _) in &fields {
            if !labels.insert(name.as_str()) {
                duplicate = Some(name.clone());
                break;
            }
        }
        if let Some(name) = duplicate {
            let mut to_drop: Vec<Value> = fields.into_iter().map(|(_, v)| v).collect();
            to_drop.push(candidate);
            self.drop_values(to_drop)?;
            return Err(Error::DuplicateField(name));
        }
        self.ensure_owned(iter::once(&candidate).chain(fields.iter().map(|(_, v)| v)))?;
        if let Err(error) = self
            .admit_reuse_candidate(&candidate)
            .and_then(|()| self.admit_values(fields.iter().map(|(_, v)| v)))
        {
            let mut to_drop: Vec<Value> = fields.into_iter().map(|(_, v)| v).collect();
            to_drop.push(candidate);
            self.drop_values(to_drop)?;
            return Err(error);
        }
        let reusable = match self.can_reuse_record(&candidate, fields.len()) {
            Ok(reusable) => reusable,
            Err(error) => {
                let mut to_drop: Vec<Value> = fields.into_iter().map(|(_, v)| v).collect();
                to_drop.push(candidate);
                self.drop_values(to_drop)?;
                return Err(error);
            }
        };
        if reusable {
            let Value::Object(id) = candidate else {
                unreachable!("only objects are reusable")
            };
            let old = self.replace_object_data(id.id, Data::Record(fields))?;
            self.stats.reuses += 1;
            if let Data::Record(old_fields) = old {
                self.drop_values(old_fields.into_iter().map(|(_, v)| v).collect())?;
            }
            Ok(Value::Object(id))
        } else {
            self.stats.reuse_fallbacks += 1;
            self.drop_value(candidate)?;
            self.try_allocate(Data::Record(fields))
        }
    }

    fn can_reuse(
        &self,
        candidate: &Value,
        total: usize,
        target_layout: Option<&CellLayout>,
    ) -> Result<bool, Error> {
        if !self.reuse_enabled {
            return Ok(false);
        }
        let Value::Object(id) = candidate else {
            return Ok(false);
        };
        let object = self.object(id.id)?;
        match &object.data {
            Data::ReuseToken {
                kind: ReuseKind::Tagged,
                capacity,
                layout,
            } => Ok(object.references == 1 && *capacity == total && layout.as_ref() == target_layout),
            Data::Tagged {
                tag,
                ordinary_fields,
                fields,
                layout,
            } => {
                tagged_boundary(tag, *ordinary_fields, fields.len())?;
                Ok(object.references == 1
                    && fields.len() == total
                    && layout.as_ref() == target_layout)
            }
            _ => Ok(false),
        }
    }

    fn can_reuse_record(&self, candidate: &Value, field_count: usize) -> Result<bool, Error> {
        if !self.reuse_enabled {
            return Ok(false);
        }
        let Value::Object(id) = candidate else {
            return Ok(false);
        };
        let object = self.object(id.id)?;
        match &object.data {
            Data::ReuseToken {
                kind: ReuseKind::Record,
                capacity,
                ..
            } => Ok(object.references == 1 && *capacity == field_count),
            Data::Record(fields) => Ok(object.references == 1 && fields.len() == field_count),
            _ => Ok(false),
        }
    }
}
