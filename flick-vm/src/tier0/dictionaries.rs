//! Owned dictionary entries and metered, callback-free structural views.
use std::{collections::BTreeSet, mem};

use flick_core::ir::{Lit, memory::DictionaryEntry};

use super::super::{DictionaryValue, ObjectId, charge};
use super::{Data, Error, Invocation, InvocationEnd, Local, Machine, Operation, Value, peek};

impl Machine<'_, '_> {
    pub(super) fn dictionary_operation(
        &mut self,
        binding: Local,
        operation: &Operation,
    ) -> Result<bool, Error> {
        match operation {
            Operation::Dictionary { entries } => self.dictionary(binding, entries)?,
            Operation::Method { dictionary, field } => self.method(binding, *dictionary, field)?,
            Operation::Structure { object, expected } => {
                self.structure(binding, *object, *expected)?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn dictionary(
        &mut self,
        binding: Local,
        entries: &[(String, DictionaryEntry)],
    ) -> Result<(), Error> {
        // Validate labels before taking any operand; all caller tokens remain
        // explicit frame roots if a duplicate or fuel fault interrupts this pass.
        let mut labels = BTreeSet::new();
        for (name, _) in entries {
            // Account for tree comparisons as well as reading the label.
            charge(
                &mut self.fuel,
                name.len()
                    .saturating_add(1)
                    .saturating_mul(labels.len().saturating_add(1)),
            )?;
            if !labels.insert(name) {
                return Err(Error::DuplicateField(name.clone()));
            }
        }
        self.stage(entries.iter().map(|(_, entry)| entry.local()))?;
        let values = mem::take(&mut self.scratch);
        let entries = entries
            .iter()
            .zip(values)
            .map(|((name, entry), value)| {
                let entry = match entry {
                    DictionaryEntry::Value(_) => DictionaryValue::Value(value),
                    DictionaryEntry::Getter(_) => DictionaryValue::Getter(value),
                };
                (name.clone(), entry)
            })
            .collect();
        let value = self.heap.allocate(Data::Dictionary(entries));
        self.store(binding, value)?;
        Ok(())
    }

    fn method(&mut self, binding: Local, dictionary: Local, field: &str) -> Result<(), Error> {
        let receiver = peek(
            &self.frames.last().expect("dictionary caller").locals,
            dictionary,
        )?;
        let Value::Object(token) = receiver else {
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
        let (value, getter) = selected.ok_or_else(|| Error::MissingMethod(field.into()))?;
        let value = self.heap.dup(&value)?;
        self.scratch.push(value);
        if !getter {
            let value = self.scratch.pop().expect("eager method");
            self.store(binding, value)?;
            return Ok(());
        }
        let receiver = self.heap.dup(receiver)?;
        self.scratch.push(receiver);
        let id = self.handler_identity()?;
        let mut operands = mem::take(&mut self.scratch).into_iter();
        self.invocations.insert(
            id,
            Invocation {
                callee: operands.next(),
                arguments: operands.collect(),
                end: InvocationEnd::Caller(binding),
                evidence: self.current_evidence,
            },
        );
        self.ready = Some(id);
        Ok(())
    }

    fn structure(&mut self, binding: Local, local: Local, expected: usize) -> Result<(), Error> {
        let receiver = peek(&self.frames.last().expect("structure caller").locals, local)?;
        let (object, record, count) = match receiver {
            Value::Unit => (None, false, 0),
            Value::Object(token) => match &self.heap.object(token.id)?.data {
                Data::Record(fields) => (Some(token.id), true, fields.len()),
                Data::Tagged {
                    tag,
                    ordinary_fields,
                    fields,
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
        let mut indices = Vec::new();
        for index in 0..count {
            charge(&mut self.fuel, 1)?;
            indices.push(index);
        }
        if record {
            self.sort_fields(object.expect("record object"), &mut indices)?;
        }
        // Every retained element is rooted in scratch before another fallible
        // step. No Rust temporary owns language tokens across a fuel check.
        for index in &indices {
            charge(&mut self.fuel, 1)?;
            let object = object.expect("nonempty structure");
            let value = match &self.heap.object(object)?.data {
                Data::Record(fields) => fields[*index].1.representation(),
                Data::Tagged { fields, .. } => fields[*index].representation(),
                _ => unreachable!("validated structure"),
            };
            self.scratch.push(self.heap.dup(&value)?);
        }
        charge(&mut self.fuel, 1)?;
        let values = mem::take(&mut self.scratch);
        let values = self.heap.construct("$tuple", values)?;
        self.scratch.push(values);
        for index in &indices {
            let value = if record {
                let Data::Record(fields) = &self.heap.object(object.expect("record object"))?.data
                else {
                    unreachable!("validated record")
                };
                charge(&mut self.fuel, fields[*index].0.len().saturating_add(1))?;
                let label = Lit::String(fields[*index].0.clone());
                self.heap.literal(&label)?
            } else {
                charge(&mut self.fuel, 1)?;
                Value::Unit
            };
            self.scratch.push(value);
        }
        charge(&mut self.fuel, 1)?;
        let labels = self.scratch.split_off(1);
        let labels = self.heap.construct("$tuple", labels)?;
        self.scratch.push(labels);
        charge(&mut self.fuel, 1)?;
        self.scratch.insert(0, Value::Bool(record));
        let fields = mem::take(&mut self.scratch);
        let value = self.heap.construct("$tuple", fields)?;
        self.store(binding, value)?;
        Ok(())
    }

    fn sort_fields(&mut self, object: ObjectId, indices: &mut [usize]) -> Result<(), Error> {
        let Data::Record(fields) = &self.heap.object(object)?.data else {
            unreachable!("record field order")
        };
        // An explicit insertion sort meters each comparison and movement.
        // UTF-8 byte order differs from UTF-16 for astral versus BMP labels.
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
