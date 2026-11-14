/// Checks internal metadata before any semantic prefix slicing or suffix
/// arithmetic. Destruction deliberately walks ALL physical fields instead.
fn tagged_boundary(tag: &str, ordinary: usize, physical: usize) -> Result<(), Error> {
    if ordinary > physical {
        return Err(Error::InvalidTaggedBoundary { ordinary, physical });
    }
    if ordinary != physical && matches!(tag, "$tuple" | "$list" | "$match") {
        return Err(Error::ReservedConstructorEvidence(tag.into()));
    }
    Ok(())
}

/// Charges actual interpreter work before performing it. Cleanup never
/// needs fuel, and exhaustion does not consume more than remains.
fn charge(fuel: &mut u64, amount: usize) -> Result<(), Error> {
    let amount = u64::try_from(amount).map_err(|_| Error::OutOfFuel)?;
    *fuel = fuel.checked_sub(amount).ok_or(Error::OutOfFuel)?;
    Ok(())
}

type Locals = BTreeMap<Local, Vec<Value>>;

/// Executes a validated ownership function. No automatic cleanup is done
/// on success or error: leaks remain observable in the heap statistics.
pub fn execute(
    heap: &mut Heap,
    function: &Function,
    arguments: Vec<Value>,
) -> Result<Value, Error> {
    memory::validate(function).map_err(Error::InvalidFunction)?;
    if function.params.len() != arguments.len() {
        return Err(Error::ArgumentCount {
            expected: function.params.len(),
            actual: arguments.len(),
        });
    }
    heap.ensure_owned(&arguments)?;
    let mut locals: Locals = function
        .params
        .iter()
        .copied()
        .zip(arguments.into_iter().map(|v| vec![v]))
        .collect();
    let mut block = &function.body;
    loop {
        for instruction in &block.instructions {
            match instruction {
                Instruction::Let { binding, value } => {
                    let value = operation(heap, &mut locals, value)?;
                    locals.insert(*binding, vec![value]);
                }
                Instruction::Dup(local) => {
                    let value = heap.dup(peek(&locals, *local)?)?;
                    locals.entry(*local).or_default().push(value);
                }
                Instruction::Drop(local) => heap.drop_value(take(&mut locals, *local)?)?,
            }
        }
        match &block.exit {
            Exit::Return(local) => {
                let result = take(&mut locals, *local)?;
                if locals.is_empty() {
                    return Ok(result);
                }
                return Err(Error::Leftover(
                    locals
                        .iter()
                        .map(|(id, tokens)| (*id, tokens.len()))
                        .collect(),
                ));
            }
            Exit::Branch {
                condition,
                then_,
                else_,
            } => {
                let Value::Bool(condition) = take(&mut locals, *condition)? else {
                    return Err(Error::ExpectedBoolean);
                };
                block = if condition { then_ } else { else_ };
            }
        }
    }
}

fn peek(locals: &Locals, local: Local) -> Result<&Value, Error> {
    locals
        .get(&local)
        .and_then(|values| values.last())
        .ok_or(Error::NoToken(local))
}

fn take(locals: &mut Locals, local: Local) -> Result<Value, Error> {
    let values = locals.get_mut(&local).ok_or(Error::NoToken(local))?;
    let value = values.pop().ok_or(Error::NoToken(local))?;
    if values.is_empty() {
        locals.remove(&local);
    }
    Ok(value)
}

fn operation(heap: &mut Heap, locals: &mut Locals, value: &Operation) -> Result<Value, Error> {
    for local in value.consumed().into_iter().chain(value.borrowed()) {
        heap.admit_ordinary(peek(locals, local)?)?;
    }
    let operands = |locals: &mut Locals, fields: &[Local]| {
        fields
            .iter()
            .map(|local| take(locals, *local))
            .collect::<Result<Vec<_>, _>>()
    };
    match value {
        Operation::ServiceTag(key) => heap.service_tag(key),
        Operation::Literal(literal) => heap.literal(literal),
        Operation::Construct { tag, fields } => heap.construct(tag, operands(locals, fields)?),
        Operation::ConstructWithDicts { tag, fields, dicts } => {
            heap.construct_with_dicts(tag, operands(locals, fields)?, operands(locals, dicts)?)
        }
        Operation::Project { object, field } => heap.project(peek(locals, *object)?, *field),
        Operation::Record { fields } => {
            let values = fields
                .iter()
                .map(|(name, local)| Ok((name.clone(), take(locals, *local)?)))
                .collect::<Result<Vec<_>, Error>>()?;
            heap.record(values)
        }
        Operation::Field { object, field } => heap.project_named(peek(locals, *object)?, field),
        Operation::Binary { op, left, right } => {
            let left = take(locals, *left)?;
            let right = take(locals, *right)?;
            let result = primitives::binary(heap, *op, &left, &right)?;
            heap.drop_values(vec![left, right])?;
            Ok(result)
        }
        Operation::Not(local) => {
            let value = take(locals, *local)?;
            let result = primitives::not(&value)?;
            heap.drop_value(value)?;
            Ok(result)
        }
        Operation::Neg { num, value } => {
            let value = take(locals, *value)?;
            let result = primitives::neg(heap, *num, &value)?;
            heap.drop_value(value)?;
            Ok(result)
        }
        Operation::RecoveryResult { .. }
        | Operation::Dictionary { .. }
        | Operation::Method { .. }
        | Operation::Structure { .. }
        | Operation::Closure { .. }
        | Operation::Match { .. }
        | Operation::Update { .. }
        | Operation::Task { .. }
        | Operation::Await(_)
        | Operation::Apply { .. }
        | Operation::Call { .. }
        | Operation::Within { .. }
        | Operation::ReadOwnerEvidence
        | Operation::WithOwnerEvidence { .. }
        | Operation::Global(_)
        | Operation::Intrinsic { .. }
        | Operation::Handle { .. }
        | Operation::Perform { .. } => Err(Error::NeedsProgram),
        Operation::Reuse {
            object,
            tag,
            fields,
        } => {
            let candidate = take(locals, *object)?;
            heap.reuse(candidate, tag, operands(locals, fields)?)
        }
        Operation::ReuseWithDicts {
            object,
            tag,
            fields,
            dicts,
        } => {
            let candidate = take(locals, *object)?;
            heap.reuse_with_dicts(
                candidate,
                tag,
                operands(locals, fields)?,
                operands(locals, dicts)?,
            )
        }
    }
}
