//! Dictionary construction, method lookup, and structural view execution for tier 1.

use std::collections::BTreeSet;
use std::mem;

use flick_core::ir::Lit;

use crate::tier0::{Data, DictionaryValue, Error, ObjectId, Value};

use super::machine::{End, Machine, RegVal};

fn charge(fuel: &mut u64, amount: usize) -> Result<(), Error> {
    let cost = u64::try_from(amount).unwrap_or(u64::MAX);
    *fuel = fuel.checked_sub(cost).ok_or(Error::OutOfFuel)?;
    Ok(())
}

impl Machine<'_, '_> {
    pub(super) fn exec_dictionary(
        &mut self,
        dst: usize,
        entries: &[(usize, usize, bool)],
    ) -> Result<(), Error> {
        let mut labels = BTreeSet::new();
        for &(field_id, _, _) in entries {
            let name = &self.image.dictionary_fields[field_id];
            charge(
                &mut self.fuel,
                name.len()
                    .saturating_add(1)
                    .saturating_mul(labels.len().saturating_add(1)),
            )?;
            if !labels.insert(name.as_str()) {
                return Err(Error::DuplicateField(name.clone()));
            }
        }
        for &(_, slot, _) in entries {
            let val = self.take_value(slot)?;
            self.scratch.push(val);
        }
        self.heap.admit_values(&self.scratch)?;
        let values = mem::take(&mut self.scratch);
        let built = entries
            .iter()
            .zip(values)
            .map(|(&(field_id, _, is_getter), value)| {
                let entry = if is_getter {
                    DictionaryValue::Getter(value)
                } else {
                    DictionaryValue::Value(value)
                };
                (self.image.dictionary_fields[field_id].clone(), entry)
            })
            .collect();
        let val = self.heap.allocate(Data::Dictionary(built));
        self.store_raw(dst, RegVal::Value(val));
        Ok(())
    }

    pub(super) fn exec_method(
        &mut self,
        dst: usize,
        dictionary: usize,
        field_id: usize,
    ) -> Result<(), Error> {
        let field = &self.image.dictionary_fields[field_id];
        let receiver = self.peek_value(dictionary)?.representation();
        let Value::Object(token) = &receiver else {
            return Err(Error::ExpectedDictionary);
        };
        let Data::Dictionary(entries) = &self.heap.object(token.id)?.data else {
            return Err(Error::ExpectedDictionary);
        };
        let mut selected = None;
        for (name, entry) in entries {
            charge(
                &mut self.fuel,
                name.len().saturating_add(field.len()).saturating_add(1),
            )?;
            if name == field {
                selected = Some((
                    entry.value().representation(),
                    matches!(entry, DictionaryValue::Getter(_)),
                ));
                break;
            }
        }
        let (val_rep, is_getter) = selected.ok_or_else(|| Error::MissingMethod(field.clone()))?;
        let val = self.heap.dup(&val_rep)?;
        if !is_getter {
            return self.store_reg(dst, RegVal::Value(val));
        }
        self.scratch.push(val);
        let recv_dup = self.heap.dup(&receiver)?;
        let getter = self.scratch.pop().expect("staged getter");
        self.apply_reg(
            RegVal::Value(getter),
            RegVal::Value(recv_dup),
            End::Store(dst),
            false,
        )
    }

    pub(super) fn exec_structure(
        &mut self,
        dst: usize,
        object: usize,
        expected: usize,
    ) -> Result<(), Error> {
        let receiver = self.peek_value(object)?;
        let (obj_id, record, count) = match receiver {
            Value::Unit => (None, false, 0),
            Value::Object(token) => match &self.heap.object(token.id)?.data {
                Data::Record(fields) => (Some(token.id), true, fields.len()),
                Data::Tagged {
                    tag,
                    ordinary_fields,
                    fields,
                    ..
                } if tag == "$tuple" && *ordinary_fields == fields.len() => {
                    (Some(token.id), false, fields.len())
                }
                _ => return Err(Error::ExpectedStructure),
            },
            _ => return Err(Error::ExpectedStructure),
        };
        if count != expected {
            return Err(Error::StructureArity {
                expected,
                actual: count,
            });
        }
        let mut indices = Vec::with_capacity(count);
        for index in 0..count {
            charge(&mut self.fuel, 1)?;
            indices.push(index);
        }
        if record {
            self.sort_record_fields(obj_id.expect("record object"), &mut indices)?;
        }
        for &index in &indices {
            charge(&mut self.fuel, 1)?;
            let id = obj_id.expect("nonempty structure");
            let val_rep = match &self.heap.object(id)?.data {
                Data::Record(fields) => fields[index].1.representation(),
                Data::Tagged { fields, .. } => fields[index].representation(),
                _ => unreachable!("validated structure"),
            };
            let dup = self.heap.dup(&val_rep)?;
            self.scratch.push(dup);
        }
        charge(&mut self.fuel, 1)?;
        let values = self
            .heap
            .construct("$tuple", mem::take(&mut self.scratch))?;
        self.scratch.push(values);
        for &index in &indices {
            let label = if record {
                let name = {
                    let Data::Record(fields) = &self.heap.object(obj_id.expect("record"))?.data
                    else {
                        unreachable!("validated record");
                    };
                    charge(&mut self.fuel, fields[index].0.len().saturating_add(1))?;
                    fields[index].0.clone()
                };
                self.heap.literal(&Lit::String(name))?
            } else {
                charge(&mut self.fuel, 1)?;
                Value::Unit
            };
            self.scratch.push(label);
        }
        charge(&mut self.fuel, 1)?;
        let labels = self.heap.construct("$tuple", self.scratch.split_off(1))?;
        self.scratch.push(labels);
        charge(&mut self.fuel, 1)?;
        self.scratch.insert(0, Value::Bool(record));
        let tuple = self
            .heap
            .construct("$tuple", mem::take(&mut self.scratch))?;
        self.store_reg(dst, RegVal::Value(tuple))
    }

    fn sort_record_fields(&mut self, object: ObjectId, indices: &mut [usize]) -> Result<(), Error> {
        let Data::Record(fields) = &self.heap.object(object)?.data else {
            unreachable!("record field order");
        };
        for at in 1..indices.len() {
            let mut at = at;
            while at > 0 {
                let left = &fields[indices[at - 1]].0;
                let right = &fields[indices[at]].0;
                charge(
                    &mut self.fuel,
                    left.len().saturating_add(right.len()).saturating_add(1),
                )?;
                if left.encode_utf16().cmp(right.encode_utf16()).is_le() {
                    break;
                }
                indices.swap(at - 1, at);
                at -= 1;
            }
        }
        Ok(())
    }
}
