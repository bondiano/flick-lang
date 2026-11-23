//! Metered immutable record updates with in-place unique-owner reuse.
use std::mem;

use flick_core::ir::memory::Local;

use super::{Data, Error, Heap, Value, charge};

enum UpdateSource {
    Existing(usize),
    Replacement(usize),
}

pub(super) fn update(
    heap: &mut Heap,
    operands: &mut Vec<Value>,
    fields: &[(String, Local)],
    fuel: &mut u64,
) -> Result<Value, Error> {
    let Value::Object(token) = &operands[0] else {
        return Err(Error::ExpectedRecord);
    };
    let object = token.id;
    let Data::Record(base) = &heap.object(object)?.data else {
        return Err(Error::ExpectedRecord);
    };
    for (name, _) in fields {
        let mut found = false;
        for (label, _) in base {
            charge(
                fuel,
                name.len().saturating_add(label.len()).saturating_add(1),
            )?;
            if name == label {
                found = true;
                break;
            }
        }
        if !found {
            return Err(Error::MissingNamedField(name.clone()));
        }
    }
    let mut sources = Vec::new();
    for (index, (label, _)) in base.iter().enumerate() {
        let mut source = UpdateSource::Existing(index);
        for (replacement, (name, _)) in fields.iter().enumerate().rev() {
            charge(
                fuel,
                name.len().saturating_add(label.len()).saturating_add(1),
            )?;
            if name == label {
                source = UpdateSource::Replacement(replacement + 1);
                break;
            }
        }
        charge(fuel, label.len().saturating_add(1))?;
        sources.push((label.clone(), source));
    }
    if heap.reuse_enabled && heap.object(object)?.references == 1 {
        // Charge commit work before any mutation so fuel exhaustion leaves
        // the base record intact and all operands cleanly droppable.
        charge(fuel, sources.len().saturating_add(1))?;
        let base_value = mem::replace(&mut operands[0], Value::Unit);
        let mut overwritten = Vec::new();
        {
            let Data::Record(base_fields) = &mut heap.object_mut(object)?.data else {
                unreachable!("validated record object")
            };
            for (index, (_, source)) in sources.iter().enumerate() {
                if let UpdateSource::Replacement(rep_index) = source {
                    let replacement = mem::replace(&mut operands[*rep_index], Value::Unit);
                    let old = mem::replace(&mut base_fields[index].1, replacement);
                    overwritten.push(old);
                }
            }
        }
        operands.extend(overwritten);
        heap.stats.reuses += 1;
        return Ok(base_value);
    }
    let checkpoint = operands.len();
    for (_, source) in &sources {
        charge(fuel, 1)?;
        let value = match source {
            UpdateSource::Existing(index) => {
                let Data::Record(base_fields) = &heap.object(object)?.data else {
                    return Err(Error::ExpectedRecord);
                };
                base_fields
                    .get(*index)
                    .map(|(_, value)| value)
                    .ok_or(Error::MissingField {
                        object,
                        field: *index,
                    })?
            }
            UpdateSource::Replacement(index) => &operands[*index],
        }
        .representation();
        operands.push(heap.dup(&value)?);
    }
    charge(fuel, 1)?;
    let values = operands.split_off(checkpoint);
    heap.stats.reuse_fallbacks += 1;
    Ok(heap.allocate(Data::Record(
        sources
            .into_iter()
            .map(|(label, _)| label)
            .zip(values)
            .collect(),
    )))
}
