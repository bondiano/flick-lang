impl Heap {
    /// Transfers ownership of `fields` into an unboxed aggregate value without
    /// allocating a counted heap cell.
    pub fn construct_aggregate(&mut self, tag: &str, fields: Vec<Value>) -> Result<Value, Error> {
        self.construct_with_rep_and_layout(tag, fields, vec![], ValueRep::Aggregate, None)
    }

    /// Transfers ordinary fields and dictionary evidence into an unboxed
    /// aggregate value without allocating a counted heap cell.
    pub fn construct_aggregate_with_dicts(
        &mut self,
        tag: &str,
        fields: Vec<Value>,
        dicts: Vec<Value>,
    ) -> Result<Value, Error> {
        self.construct_with_rep_and_layout(tag, fields, dicts, ValueRep::Aggregate, None)
    }

    /// Allocates a tagged heap cell annotated with an abstract layout descriptor.
    pub fn construct_with_layout(
        &mut self,
        tag: &str,
        fields: Vec<Value>,
        layout: Option<CellLayout>,
    ) -> Result<Value, Error> {
        self.construct_with_rep_and_layout(tag, fields, vec![], ValueRep::Cell, layout)
    }

    /// Transfers both vectors into either a counted `Data::Tagged` cell or an
    /// unboxed `Value::Aggregate`, validating ownership and field boundaries.
    pub fn construct_with_rep_and_layout(
        &mut self,
        tag: &str,
        mut fields: Vec<Value>,
        dicts: Vec<Value>,
        rep: ValueRep,
        layout: Option<CellLayout>,
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
        match rep {
            ValueRep::Cell => Ok(self.allocate(Data::Tagged {
                tag: tag.to_owned(),
                ordinary_fields,
                fields,
                layout,
            })),
            ValueRep::Aggregate => Ok(Value::Aggregate {
                tag: tag.to_owned(),
                ordinary_fields,
                fields,
            }),
        }
    }

    /// Consumes an unboxed aggregate and transfers its references into a boxed
    /// fallback cell with zero extra descendant `dup`s.
    pub fn box_aggregate(
        &mut self,
        value: Value,
        layout: Option<CellLayout>,
    ) -> Result<Value, Error> {
        self.ensure_owned(iter::once(&value))?;
        if let Err(error) = self.admit_ordinary(&value) {
            self.drop_value(value)?;
            return Err(error);
        }
        if !matches!(value, Value::Aggregate { .. }) {
            self.drop_value(value)?;
            return Err(Error::ExpectedAggregate);
        }
        Ok(self.allocate(Data::BoxedAggregate {
            layout,
            value: Box::new(value),
        }))
    }

    /// Borrows a boxed fallback cell and returns an owned copy of its
    /// aggregate, retaining all boxed descendants.
    pub fn unbox_borrow(&mut self, object: &Value) -> Result<Value, Error> {
        self.admit_ordinary(object)?;
        let Value::Object(token) = object else {
            return Err(Error::ExpectedBoxedAggregate);
        };
        let Data::BoxedAggregate { value, .. } = &self.object(token.id)?.data else {
            return Err(Error::ExpectedBoxedAggregate);
        };
        let copy = value.representation();
        self.dup(&copy)
    }

    /// Consumes a boxed fallback cell: transfers its aggregate and frees the
    /// cell with zero extra descendant `dup`/`drop` when unique, or retains
    /// boxed descendants before releasing the cell reference when shared.
    pub fn unbox_consume(&mut self, object: Value) -> Result<Value, Error> {
        self.ensure_owned(iter::once(&object))?;
        if let Err(error) = self.admit_ordinary(&object) {
            self.drop_value(object)?;
            return Err(error);
        }
        let Value::Object(token) = object else {
            self.drop_value(object)?;
            return Err(Error::ExpectedBoxedAggregate);
        };
        let unique = {
            let slot_object = self.object(token.id)?;
            if !matches!(slot_object.data, Data::BoxedAggregate { .. }) {
                self.drop_value(Value::Object(token))?;
                return Err(Error::ExpectedBoxedAggregate);
            }
            slot_object.references == 1
        };
        if unique {
            let slot = &mut self.objects[token.id.index];
            let freed = slot.object.take().expect("verified boxed aggregate slot");
            slot.generation = slot.generation.wrapping_add(1);
            self.free_list.push(token.id.index);
            self.stats.drops += 1;
            self.stats.frees += 1;
            self.stats.live_objects -= 1;
            let Data::BoxedAggregate { value, .. } = freed.data else {
                unreachable!("verified boxed aggregate");
            };
            Ok(*value)
        } else {
            let copy = {
                let Data::BoxedAggregate { value, .. } = &self.object(token.id)?.data else {
                    unreachable!("verified boxed aggregate");
                };
                value.representation()
            };
            let owned = match self.dup(&copy) {
                Ok(owned) => owned,
                Err(error) => {
                    self.drop_value(Value::Object(token))?;
                    return Err(error);
                }
            };
            self.drop_value(Value::Object(token))?;
            Ok(owned)
        }
    }

    /// Returns the abstract layout metadata stored on a counted tagged cell,
    /// reuse token, or boxed fallback cell.
    pub fn cell_layout(&self, value: &Value) -> Result<Option<&CellLayout>, Error> {
        let Value::Object(token) = value else {
            return Ok(None);
        };
        Ok(match &self.object(token.id)?.data {
            Data::Tagged { layout, .. }
            | Data::ReuseToken { layout, .. }
            | Data::BoxedAggregate { layout, .. } => layout.as_ref(),
            _ => None,
        })
    }
}

fn aggregate_field(value: &Value, field: usize) -> Result<&Value, Error> {
    let Value::Aggregate {
        tag,
        ordinary_fields,
        fields,
    } = value
    else {
        return Err(Error::ExpectedObject);
    };
    tagged_boundary(tag, *ordinary_fields, fields.len())?;
    fields.get(field).ok_or(Error::MissingAggregateField(field))
}

fn aggregate_tag(value: &Value) -> Result<&str, Error> {
    let Value::Aggregate {
        tag,
        ordinary_fields,
        fields,
    } = value
    else {
        return Err(Error::ExpectedObject);
    };
    tagged_boundary(tag, *ordinary_fields, fields.len())?;
    Ok(tag)
}
