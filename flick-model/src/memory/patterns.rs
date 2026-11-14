//! Metered two-phase pattern selection and immutable record updates.
//! Selection contains only nonowning locations. Failed alternatives retain
//! nothing and allocate no language objects. Commit roots every owned token
//! in the machine scratch vector before the next fallible/fuelled step.
use std::mem;

use flick_core::ir::memory::Local;
use flick_core::ir::{Lit, Pat};

use super::{Data, Error, Heap, ObjectId, Value, charge, primitives, tagged_boundary};

#[derive(Clone, Copy)]
enum Place {
    Root,
    Tagged { object: ObjectId, index: usize },
    Record { object: ObjectId, index: usize },
    Tail { object: ObjectId, start: usize },
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
    let (object, start, length) = if let Place::Tail { object, start } = place {
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } = &heap.object(object)?.data
        else {
            return Err(Error::PatternKind("list"));
        };
        tagged_boundary(tag, *ordinary_fields, fields.len())?;
        if tag != "$list" {
            return Err(Error::PatternKind("list"));
        }
        (object, start, fields.len() - start)
    } else {
        let (object, tag, fields) = tagged(heap, root, place, "list")?;
        if tag != "$list" {
            return Err(Error::PatternKind("list"));
        }
        (object, 0, fields.len())
    };
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
        (Lit::Float(left), Value::Float(right)) => Ok(left == right),
        (Lit::String(left), _) => {
            let right = heap.string(value)?;
            charge(fuel, left.len().saturating_add(right.len()))?;
            Ok(left == right)
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
    if let Place::Tail { object, start } = place {
        let Data::Tagged {
            tag,
            ordinary_fields,
            fields,
        } = &heap.object(object)?.data
        else {
            return Err(Error::PatternKind("list"));
        };
        tagged_boundary(tag, *ordinary_fields, fields.len())?;
        if tag != "$list" {
            return Err(Error::PatternKind("list"));
        }
        let length = fields.len();
        let checkpoint = scratch.len();
        for index in start..length {
            charge(fuel, 1)?;
            let value = actual(heap, root, Place::Tagged { object, index })?.representation();
            scratch.push(heap.dup(&value)?);
        }
        charge(fuel, 1)?;
        let tail = heap.allocate(Data::Tagged {
            tag: "$list".into(),
            ordinary_fields: scratch.len() - checkpoint,
            fields: scratch.split_off(checkpoint),
        });
        scratch.push(tail);
    } else {
        charge(fuel, 1)?;
        let value = actual(heap, root, place)?.representation();
        scratch.push(heap.dup(&value)?);
    }
    Ok(())
}

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
    let checkpoint = operands.len();
    for (_, source) in &sources {
        charge(fuel, 1)?;
        let value = match source {
            UpdateSource::Existing(index) => actual(
                heap,
                &Value::Unit,
                Place::Record {
                    object,
                    index: *index,
                },
            )?,
            UpdateSource::Replacement(index) => &operands[*index],
        }
        .representation();
        operands.push(heap.dup(&value)?);
    }
    charge(fuel, 1)?;
    let values = operands.split_off(checkpoint);
    Ok(heap.allocate(Data::Record(
        sources
            .into_iter()
            .map(|(label, _)| label)
            .zip(values)
            .collect(),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_partial_tail_selection_has_no_counted_allocations_or_retains() {
        let mut heap = Heap::new();
        let text = heap
            .literal(&Lit::String("kept only by input".into()))
            .expect("text");
        let list = heap.construct("$list", vec![text]).expect("list");
        let input = heap
            .construct("$tuple", vec![list, Value::Bool(true)])
            .expect("pair");
        let pattern = Pat::Tuple(vec![
            Pat::List(vec![], Some(Box::new(Pat::Bind("tail".into())))),
            Pat::Lit(Lit::Bool(false)),
        ]);
        let before = heap.stats();
        let selected = select(&heap, &input, &pattern, &mut 1000).expect("selection");
        assert!(selected.is_none());
        assert_eq!(
            heap.stats(),
            before,
            "selection never creates an owned token or a bound tail"
        );
        let mut scratch = vec![];
        let result = execute(&mut heap, &input, &pattern, &mut 1000, &mut scratch)
            .expect("failed match result");
        assert_eq!(
            heap.stats().allocations,
            before.allocations + 1,
            "only the false result object allocates"
        );
        assert_eq!(heap.stats().dups, before.dups);
        assert!(scratch.is_empty());
        heap.drop_values(vec![input, result])
            .expect("explicit roots release");
        heap.finish().expect("no failed-tail storage");
    }
    #[test]
    fn update_preserves_base_field_order_and_takes_last_duplicate() {
        let mut heap = Heap::new();
        let base = heap
            .record(vec![
                ("z".into(), Value::Int(1)),
                ("a".into(), Value::Int(2)),
            ])
            .expect("record");
        let mut operands = vec![base, Value::Int(7), Value::Int(8), Value::Int(9)];
        let fields = vec![("a".into(), 1), ("z".into(), 2), ("a".into(), 3)];
        let result = update(&mut heap, &mut operands, &fields, &mut 1000).expect("record update");
        let Value::Object(token) = &result else {
            panic!("updated record")
        };
        let Data::Record(fields) = &heap.object(token.id).expect("record allocation").data else {
            panic!("record storage")
        };
        assert_eq!(
            fields,
            &[("z".into(), Value::Int(8)), ("a".into(), Value::Int(9))]
        );
        operands.push(result);
        heap.drop_values(operands)
            .expect("staged operands and output release");
        heap.finish().expect("no update leak");
    }
    #[test]
    fn deferred_tail_materialization_checks_reserved_collection_metadata() {
        let mut heap = Heap::new();
        let hidden = heap
            .literal(&Lit::String("hidden owned field".into()))
            .expect("hidden field");
        let root = heap.allocate(Data::Tagged {
            tag: "$list".into(),
            ordinary_fields: 1,
            fields: vec![Value::Int(1), hidden],
        });
        let object = root.object_id().expect("list identity");
        let mut scratch = vec![];
        assert_eq!(
            materialize(
                &mut heap,
                &root,
                Place::Tail { object, start: 0 },
                &mut 1000,
                &mut scratch
            ),
            Err(Error::ReservedConstructorEvidence("$list".into()))
        );
        assert!(scratch.is_empty());
        assert_eq!(heap.stats().dups, 0);
        heap.drop_value(root)
            .expect("all physical list fields release");
        heap.finish()
            .expect("rejected tail has no owned temporaries");
    }
}
