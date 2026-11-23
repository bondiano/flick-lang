//! Metered two-phase pattern selection.
//! Selection contains only nonowning locations. Failed alternatives retain
//! nothing and allocate no language objects. Commit roots every owned token
//! in the machine scratch vector before the next fallible/fuelled step.
use std::mem;

use flick_core::ir::{EnumLit, JsClass, Lit, Pat, Rep};

pub(super) use super::record_update::update;
use super::{Data, Error, Heap, ObjectId, ObjectToken, Value, charge, primitives, tagged_boundary};

#[derive(Clone, Copy)]
enum Place {
    Root,
    Tagged {
        object: ObjectId,
        index: usize,
    },
    Record {
        object: ObjectId,
        index: usize,
    },
    Tail {
        object: ObjectId,
        start: usize,
        end: usize,
    },
}

enum Work<'p> {
    Check(&'p Pat, Place),
    EndOr,
}

struct Choice<'p> {
    alternatives: &'p [Pat],
    next: usize,
    place: Place,
    work: usize,
    bindings: usize,
}

type Bindings<'p> = Vec<(&'p str, Place)>;

pub(super) fn execute(
    heap: &mut Heap,
    root: &Value,
    pattern: &Pat,
    fuel: &mut u64,
    scratch: &mut Vec<Value>,
) -> Result<Value, Error> {
    heap.admit_ordinary(root)?;
    let Some(mut bindings) = select(heap, root, pattern, fuel)? else {
        charge(fuel, 1)?;
        return Ok(heap.allocate(Data::Tagged {
            tag: "$match".into(),
            ordinary_fields: 1,
            fields: vec![Value::Bool(false)],
        }));
    };
    // Insertion sort charges each lexical comparison. This avoids an
    // unmetered library sort or an assumed comparator-work bound.
    for at in 1..bindings.len() {
        let mut at = at;
        while at > 0 {
            charge(
                fuel,
                bindings[at - 1]
                    .0
                    .len()
                    .saturating_add(bindings[at].0.len())
                    .saturating_add(1),
            )?;
            if bindings[at - 1].0 <= bindings[at].0 {
                break;
            }
            bindings.swap(at - 1, at);
            at -= 1;
        }
    }
    scratch.push(Value::Bool(true));
    for (_, place) in bindings {
        materialize(heap, root, place, fuel, scratch)?;
    }
    charge(fuel, 1)?;
    Ok(heap.allocate(Data::Tagged {
        tag: "$match".into(),
        ordinary_fields: scratch.len(),
        fields: mem::take(scratch),
    }))
}

fn select<'p>(
    heap: &Heap,
    root: &Value,
    pattern: &'p Pat,
    fuel: &mut u64,
) -> Result<Option<Bindings<'p>>, Error> {
    let mut work = vec![Work::Check(pattern, Place::Root)];
    let mut choices = Vec::new();
    let mut bindings = Vec::new();
    while let Some(next) = work.pop() {
        charge(fuel, 1)?;
        let matched = match next {
            Work::EndOr => {
                choices.pop().expect("completed Or checkpoint");
                true
            }
            Work::Check(Pat::Wild, _) => true,
            Work::Check(Pat::Bind(name), place) => {
                bindings.push((name.as_str(), place));
                true
            }
            Work::Check(Pat::Or(alternatives), place) => {
                choices.push(Choice {
                    alternatives,
                    next: 1,
                    place,
                    work: work.len(),
                    bindings: bindings.len(),
                });
                work.push(Work::EndOr);
                work.push(Work::Check(&alternatives[0], place));
                true
            }
            Work::Check(pattern, place) => check(heap, root, pattern, place, fuel, &mut work)?,
        };
        if !matched && !retry(&mut choices, &mut work, &mut bindings, fuel)? {
            return Ok(None);
        }
    }
    Ok(Some(bindings))
}

fn retry<'p>(
    choices: &mut Vec<Choice<'p>>,
    work: &mut Vec<Work<'p>>,
    bindings: &mut Bindings<'p>,
    fuel: &mut u64,
) -> Result<bool, Error> {
    while let Some(choice) = choices.last_mut() {
        charge(fuel, 1)?;
        work.truncate(choice.work);
        bindings.truncate(choice.bindings);
        if let Some(pattern) = choice.alternatives.get(choice.next) {
            choice.next += 1;
            work.push(Work::EndOr);
            work.push(Work::Check(pattern, choice.place));
            return Ok(true);
        }
        choices.pop();
    }
    Ok(false)
}

fn actual<'a>(heap: &'a Heap, root: &'a Value, place: Place) -> Result<&'a Value, Error> {
    match place {
        Place::Root => Ok(root),
        Place::Tagged { object, index } => {
            let Data::Tagged { fields, .. } = &heap.object(object)?.data else {
                return Err(Error::ExpectedObject);
            };
            fields.get(index).ok_or(Error::MissingField {
                object,
                field: index,
            })
        }
        Place::Record { object, index } => {
            let Data::Record(fields) = &heap.object(object)?.data else {
                return Err(Error::ExpectedRecord);
            };
            fields
                .get(index)
                .map(|(_, value)| value)
                .ok_or(Error::MissingField {
                    object,
                    field: index,
                })
        }
        Place::Tail { .. } => Err(Error::PatternKind("non-list value")),
    }
}

fn tagged<'a>(
    heap: &'a Heap,
    root: &'a Value,
    place: Place,
    kind: &'static str,
) -> Result<(ObjectId, &'a str, &'a [Value]), Error> {
    let Value::Object(token) = actual(heap, root, place)? else {
        return Err(Error::PatternKind(kind));
    };
    let Data::Tagged {
        tag,
        ordinary_fields,
        fields,
    } = &heap.object(token.id)?.data
    else {
        return Err(Error::PatternKind(kind));
    };
    tagged_boundary(tag, *ordinary_fields, fields.len())?;
    Ok((token.id, tag, fields))
}

#[expect(
    clippy::too_many_lines,
    reason = "complete pattern check dispatch over all Rep layouts"
)]
fn check<'p>(
    heap: &Heap,
    root: &Value,
    pattern: &'p Pat,
    place: Place,
    fuel: &mut u64,
    work: &mut Vec<Work<'p>>,
) -> Result<bool, Error> {
    match pattern {
        Pat::Lit(literal) => literal_matches(heap, actual(heap, root, place)?, literal, fuel),
        Pat::Tuple(items) if items.is_empty() => match actual(heap, root, place)? {
            Value::Unit => Ok(true),
            _ => Err(Error::PatternKind("Unit")),
        },
        Pat::Tuple(items) => {
            let (object, tag, fields) = tagged(heap, root, place, "tuple")?;
            if tag != "$tuple" {
                return Err(Error::PatternKind("tuple"));
            }
            layout(items.len(), fields.len())?;
            children(items, object, 0, fuel, work)?;
            Ok(true)
        }
        Pat::List(items, rest) => list(heap, root, items, rest.as_deref(), place, fuel, work),
        Pat::Record(fields) => record(heap, root, fields, place, fuel, work),
        Pat::Con(constructor, items) => {
            match &constructor.rep {
                Rep::Literal(enum_lit) => {
                    layout(items.len(), 0)?;
                    let lit = match enum_lit {
                        EnumLit::Int(n) => Lit::Int(*n),
                        EnumLit::String(s) => Lit::String(s.clone()),
                    };
                    return literal_matches(heap, actual(heap, root, place)?, &lit, fuel);
                }
                Rep::Unboxed if constructor.arity == 1 && constructor.dicts == 0 => {
                    layout(items.len(), 1)?;
                    work.push(Work::Check(&items[0], place));
                    return Ok(true);
                }
                Rep::Typeof(class) if constructor.arity == 1 && constructor.dicts == 0 => {
                    layout(items.len(), 1)?;
                    let val = actual(heap, root, place)?;
                    let matches = match class {
                        JsClass::Number => matches!(
                            val,
                            Value::Int(_)
                                | Value::Int8(_)
                                | Value::Int16(_)
                                | Value::UInt8(_)
                                | Value::UInt16(_)
                                | Value::UInt32(_)
                                | Value::Float(_)
                                | Value::Float32(_)
                        ),
                        JsClass::String => matches!(
                            val,
                            Value::Object(tok)
                                if matches!(
                                    heap.object(tok.id)?.data,
                                    Data::String(_) | Data::Utf16(_)
                                )
                        ),
                        JsClass::Boolean => matches!(val, Value::Bool(_)),
                        JsClass::Bigint => {
                            matches!(val, Value::Int64(_) | Value::UInt64(_))
                                || matches!(
                                    val,
                                    Value::Object(tok)
                                        if matches!(heap.object(tok.id)?.data, Data::BigInt(_))
                                )
                        }
                        JsClass::Array => matches!(
                            val,
                            Value::Object(tok)
                                if matches!(&heap.object(tok.id)?.data, Data::ListSlice { .. })
                                    || matches!(
                                        &heap.object(tok.id)?.data,
                                        Data::Tagged { tag, .. }
                                            if tag == "$list" || tag == "$tuple"
                                    )
                        ),
                    };
                    if !matches {
                        return Ok(false);
                    }
                    work.push(Work::Check(&items[0], place));
                    return Ok(true);
                }
                _ => {}
            }
            let (object, tag, fields) = tagged(heap, root, place, "constructor")?;
            charge(
                fuel,
                constructor
                    .module
                    .len()
                    .saturating_add(constructor.ty.len())
                    .saturating_add(constructor.name.len())
                    .saturating_add(tag.len())
                    .saturating_add(2),
            )?;
            if tag
                != format!(
                    "{}.{}.{}",
                    constructor.module, constructor.ty, constructor.name
                )
            {
                return Ok(false);
            }
            layout(items.len(), fields.len())?;
            let Data::Tagged {
                ordinary_fields, ..
            } = &heap.object(object)?.data
            else {
                unreachable!("validated tagged object")
            };
            let actual_dicts = fields.len() - ordinary_fields;
            if *ordinary_fields != constructor.arity || actual_dicts != constructor.dicts {
                return Err(Error::PatternEvidenceLayout {
                    expected_fields: constructor.arity,
                    actual_fields: *ordinary_fields,
                    expected_dicts: constructor.dicts,
                    actual_dicts,
                });
            }
            children(items, object, 0, fuel, work)?;
            Ok(true)
        }
        Pat::Wild | Pat::Bind(_) | Pat::Or(_) => unreachable!("selection-only forms"),
    }
}

fn layout(expected: usize, actual: usize) -> Result<(), Error> {
    if expected == actual {
        Ok(())
    } else {
        Err(Error::PatternLayout { expected, actual })
    }
}

fn children<'p>(
    patterns: &'p [Pat],
    object: ObjectId,
    start: usize,
    fuel: &mut u64,
    work: &mut Vec<Work<'p>>,
) -> Result<(), Error> {
    charge(fuel, patterns.len())?;
    work.extend(patterns.iter().enumerate().rev().map(|(index, pattern)| {
        Work::Check(
            pattern,
            Place::Tagged {
                object,
                index: start + index,
            },
        )
    }));
    Ok(())
}

fn list<'p>(
    heap: &Heap,
    root: &Value,
    items: &'p [Pat],
    rest: Option<&'p Pat>,
    place: Place,
    fuel: &mut u64,
    work: &mut Vec<Work<'p>>,
) -> Result<bool, Error> {
    let (object, start, end) = if let Place::Tail { object, start, end } = place {
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } = &heap.object(object)?.data
        else {
            return Err(Error::PatternKind("list"));
        };
        tagged_boundary(tag, *ordinary_fields, fields.len())?;
        if tag != "$list" || end > fields.len() {
            return Err(Error::PatternKind("list"));
        }
        (object, start, end)
    } else {
        let value = actual(heap, root, place)?;
        let (object, offset, len) = heap.list_view(value)?;
        (object, offset, offset + len)
    };
    let length = end - start;
    if length < items.len() || rest.is_none() && length != items.len() {
        return Ok(false);
    }
    if let Some(rest) = rest {
        charge(fuel, 1)?;
        work.push(Work::Check(
            rest,
            Place::Tail {
                object,
                start: start + items.len(),
                end,
            },
        ));
    }
    children(items, object, start, fuel, work)?;
    Ok(true)
}

fn record<'p>(
    heap: &Heap,
    root: &Value,
    patterns: &'p [(String, Pat)],
    place: Place,
    fuel: &mut u64,
    work: &mut Vec<Work<'p>>,
) -> Result<bool, Error> {
    let Value::Object(token) = actual(heap, root, place)? else {
        return Err(Error::ExpectedRecord);
    };
    let Data::Record(fields) = &heap.object(token.id)?.data else {
        return Err(Error::ExpectedRecord);
    };
    for (name, pattern) in patterns.iter().rev() {
        let mut found = None;
        for (index, (label, _)) in fields.iter().enumerate() {
            charge(
                fuel,
                name.len().saturating_add(label.len()).saturating_add(1),
            )?;
            if name == label {
                found = Some(index);
                break;
            }
        }
        let index = found.ok_or_else(|| Error::MissingNamedField(name.clone()))?;
        work.push(Work::Check(
            pattern,
            Place::Record {
                object: token.id,
                index,
            },
        ));
    }
    Ok(true)
}

#[expect(
    clippy::float_cmp,
    reason = "literal Float matching uses numeric equality, not SameValueZero"
)]
fn literal_matches(
    heap: &Heap,
    value: &Value,
    literal: &Lit,
    fuel: &mut u64,
) -> Result<bool, Error> {
    match (literal, value) {
        (Lit::Unit, Value::Unit) => Ok(true),
        (Lit::Bool(left), Value::Bool(right)) => Ok(left == right),
        (Lit::Int(left), Value::Int(right)) => Ok(left == right),
        (Lit::Int8(left), Value::Int8(right)) => Ok(left == right),
        (Lit::Int16(left), Value::Int16(right)) => Ok(left == right),
        (Lit::Int64(left), Value::Int64(right)) => Ok(left == right),
        (Lit::UInt8(left), Value::UInt8(right)) => Ok(left == right),
        (Lit::UInt16(left), Value::UInt16(right)) => Ok(left == right),
        (Lit::UInt32(left), Value::UInt32(right)) => Ok(left == right),
        (Lit::UInt64(left), Value::UInt64(right)) => Ok(left == right),
        (Lit::Float(left), Value::Float(right)) => Ok(left == right),
        (Lit::Float32(left), Value::Float32(right)) => Ok(left == right),
        (Lit::String(left), _) => {
            if let Ok(right) = heap.string(value) {
                charge(fuel, left.len().saturating_add(right.len()))?;
                Ok(left == right)
            } else {
                let units = heap.utf16_units(value)?;
                charge(fuel, left.len().saturating_add(units.len()))?;
                Ok(false)
            }
        }
        (Lit::BigInt(left), _) => primitives::bigint_pattern_eq(left, heap.bigint(value)?, fuel),
        _ => Err(Error::PatternKind("matching literal type")),
    }
}

fn materialize(
    heap: &mut Heap,
    root: &Value,
    place: Place,
    fuel: &mut u64,
    scratch: &mut Vec<Value>,
) -> Result<(), Error> {
    if let Place::Tail { object, start, end } = place {
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } = &heap.object(object)?.data
        else {
            return Err(Error::PatternKind("list"));
        };
        tagged_boundary(tag, *ordinary_fields, fields.len())?;
        if tag != "$list" || end > fields.len() || start > end {
            return Err(Error::PatternKind("list"));
        }
        charge(fuel, 1)?;
        let len = end - start;
        let tail = if len == 0 {
            heap.allocate(Data::Tagged {
                tag: "$list".into(),
                ordinary_fields: 0,
                fields: Vec::new(),
            })
        } else {
            let backing = heap.dup(&Value::Object(ObjectToken { id: object }))?;
            heap.allocate(Data::ListSlice {
                backing,
                offset: start,
                len,
            })
        };
        scratch.push(tail);
    } else {
        charge(fuel, 1)?;
        let value = actual(heap, root, place)?.representation();
        scratch.push(heap.dup(&value)?);
    }
    Ok(())
}

#[cfg(test)]
#[path = "patterns_tests.rs"]
mod tests;
