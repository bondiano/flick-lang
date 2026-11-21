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

const DENSE_LOCALS: usize = 256;

#[derive(Default)]
enum LocalSlot {
    #[default]
    Empty,
    One(Value),
    Many(Vec<Value>),
}

/// Fast inline local table for dense low-index locals with sparse fallback.
#[derive(Default)]
struct Locals {
    dense: Vec<LocalSlot>,
    sparse: BTreeMap<Local, Vec<Value>>,
    live: usize,
}

impl Locals {
    fn new() -> Self {
        Self::default()
    }

    fn is_empty(&self) -> bool {
        self.live == 0
    }

    fn insert_one(&mut self, local: Local, value: Value) {
        let idx = local as usize;
        if idx < DENSE_LOCALS {
            if idx >= self.dense.len() {
                self.dense.resize_with(idx + 1, LocalSlot::default);
            }
            match &mut self.dense[idx] {
                LocalSlot::Empty => {
                    self.dense[idx] = LocalSlot::One(value);
                    self.live += 1;
                }
                LocalSlot::One(old) => {
                    *old = value;
                }
                LocalSlot::Many(vec) => {
                    let old_len = vec.len();
                    self.dense[idx] = LocalSlot::One(value);
                    self.live = self.live + 1 - old_len;
                }
            }
        } else if let Some(old) = self.sparse.insert(local, vec![value]) {
            self.live = self.live + 1 - old.len();
        } else {
            self.live += 1;
        }
    }

    fn push(&mut self, local: Local, value: Value) {
        let idx = local as usize;
        if idx < DENSE_LOCALS {
            if idx >= self.dense.len() {
                self.dense.resize_with(idx + 1, LocalSlot::default);
            }
            let slot = mem::take(&mut self.dense[idx]);
            self.dense[idx] = match slot {
                LocalSlot::Empty => LocalSlot::One(value),
                LocalSlot::One(first) => LocalSlot::Many(vec![first, value]),
                LocalSlot::Many(mut vec) => {
                    vec.push(value);
                    LocalSlot::Many(vec)
                }
            };
        } else {
            self.sparse.entry(local).or_default().push(value);
        }
        self.live += 1;
    }

    fn peek(&self, local: Local) -> Result<&Value, Error> {
        let idx = local as usize;
        if idx < DENSE_LOCALS {
            match self.dense.get(idx) {
                Some(LocalSlot::One(v)) => Ok(v),
                Some(LocalSlot::Many(vec)) => vec.last().ok_or(Error::NoToken(local)),
                _ => Err(Error::NoToken(local)),
            }
        } else {
            self.sparse
                .get(&local)
                .and_then(|values| values.last())
                .ok_or(Error::NoToken(local))
        }
    }

    fn take(&mut self, local: Local) -> Result<Value, Error> {
        let idx = local as usize;
        if idx < DENSE_LOCALS {
            let slot = self.dense.get_mut(idx).ok_or(Error::NoToken(local))?;
            match mem::take(slot) {
                LocalSlot::Empty => Err(Error::NoToken(local)),
                LocalSlot::One(v) => {
                    self.live -= 1;
                    Ok(v)
                }
                LocalSlot::Many(mut vec) => {
                    let Some(v) = vec.pop() else {
                        return Err(Error::NoToken(local));
                    };
                    self.live -= 1;
                    *slot = match vec.len() {
                        0 => LocalSlot::Empty,
                        1 => LocalSlot::One(vec.pop().expect("single remaining")),
                        _ => LocalSlot::Many(vec),
                    };
                    Ok(v)
                }
            }
        } else {
            let values = self.sparse.get_mut(&local).ok_or(Error::NoToken(local))?;
            let value = values.pop().ok_or(Error::NoToken(local))?;
            if values.is_empty() {
                self.sparse.remove(&local);
            }
            self.live -= 1;
            Ok(value)
        }
    }

    fn drain_values(&mut self, out: &mut Vec<Value>) {
        if self.live == 0 {
            return;
        }
        for slot in &mut self.dense {
            match mem::take(slot) {
                LocalSlot::Empty => {}
                LocalSlot::One(v) => out.push(v),
                LocalSlot::Many(vec) => out.extend(vec),
            }
        }
        for values in mem::take(&mut self.sparse).into_values() {
            out.extend(values);
        }
        self.live = 0;
    }

    fn into_values_vec(mut self) -> Vec<Value> {
        let mut out = Vec::with_capacity(self.live);
        self.drain_values(&mut out);
        out
    }

    fn leftover(&self) -> Vec<(Local, usize)> {
        let mut out = Vec::new();
        for (idx, slot) in self.dense.iter().enumerate() {
            let count = match slot {
                LocalSlot::Empty => 0,
                LocalSlot::One(_) => 1,
                LocalSlot::Many(vec) => vec.len(),
            };
            if count > 0 {
                out.push((Local::try_from(idx).expect("dense local fits u32"), count));
            }
        }
        for (id, values) in &self.sparse {
            if !values.is_empty() {
                out.push((*id, values.len()));
            }
        }
        out
    }
}

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
    let mut locals = Locals::new();
    for (&param, value) in function.params.iter().zip(arguments) {
        locals.insert_one(param, value);
    }
    let mut block = &function.body;
    loop {
        for instruction in &block.instructions {
            match instruction {
                Instruction::Let { binding, value } => {
                    let value = operation(heap, &mut locals, value)?;
                    locals.insert_one(*binding, value);
                }
                Instruction::Dup(local) => {
                    let value = heap.dup(peek(&locals, *local)?)?;
                    locals.push(*local, value);
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
                return Err(Error::Leftover(locals.leftover()));
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
    locals.peek(local)
}

fn take(locals: &mut Locals, local: Local) -> Result<Value, Error> {
    locals.take(local)
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
        Operation::Label(name) => Ok(heap.allocate(Data::Label(name.clone()))),
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
        | Operation::Perform { .. }
        | Operation::VarMake(_)
        | Operation::VarGet(_)
        | Operation::VarSet { .. }
        | Operation::Loop { .. }
        | Operation::LoopExit { .. }
        | Operation::FunctionBoundary { .. }
        | Operation::UnwrapOption(_)
        | Operation::GenerateStrict { .. }
        | Operation::GenerateLazy { .. }
        | Operation::Yield(_)
        | Operation::SeqOpen(_)
        | Operation::SeqNext(_) => Err(Error::NeedsProgram),
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
