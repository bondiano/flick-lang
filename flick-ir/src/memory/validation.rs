/// Checks token balance on every path, independently of the insertion
/// pass. It does not establish dynamic types, heap acyclicity, or that
/// source lowering preserves the source program's behavior.
pub fn validate(function: &Function) -> Result<(), Error> {
    let mut tokens = BTreeMap::new();
    let mut defined = BTreeSet::new();
    for &param in &function.params {
        bind(param, &mut tokens, &mut defined)?;
    }
    check(&function.body, tokens, defined)
}

/// Checks every code body and symbol/arity edge, including unselected
/// branches, before the counted interpreter mutates its heap.
pub fn validate_program(program: &Program) -> Result<(), Error> {
    let mut globals = BTreeSet::new();
    for global in &program.globals {
        if !globals.insert(&global.name) {
            return Err(Error::DuplicateGlobal(global.name.clone()));
        }
        code_arity(program, global.initializer, 0)?;
    }
    for (&code, function) in &program.functions {
        validate(function)
            .and_then(|()| symbols(&function.body, program, &globals))
            .map_err(|error| Error::InCode {
                code,
                error: Box::new(error),
            })?;
    }
    Ok(())
}

fn code_arity(program: &Program, code: CodeId, actual: usize) -> Result<(), Error> {
    let function = program
        .functions
        .get(&code)
        .ok_or(Error::UnknownCode(code))?;
    if function.params.len() == actual {
        Ok(())
    } else {
        Err(Error::Arity {
            code,
            expected: function.params.len(),
            actual,
        })
    }
}

fn symbols(block: &Block, program: &Program, globals: &BTreeSet<&Name>) -> Result<(), Error> {
    for instruction in &block.instructions {
        if let Instruction::Let { value, .. } = instruction {
            match value {
                Operation::Closure { code, .. } | Operation::Task { code, .. } => {
                    code_arity(program, *code, 2)?;
                }
                Operation::Call { code, args }
                | Operation::Within { code, args, .. }
                | Operation::WithOwnerEvidence { code, args, .. } => {
                    code_arity(program, *code, args.len())?;
                }
                Operation::Global(name) if !globals.contains(name) => {
                    return Err(Error::UnknownGlobal(name.clone()));
                }
                Operation::Intrinsic { kind, args } if args.len() != kind.arity() => {
                    return Err(Error::IntrinsicArity {
                        kind: *kind,
                        actual: args.len(),
                    });
                }
                _ => {}
            }
        }
    }
    if let Exit::Branch { then_, else_, .. } = &block.exit {
        symbols(then_, program, globals)?;
        symbols(else_, program, globals)?;
    }
    Ok(())
}

fn bind(
    local: Local,
    tokens: &mut BTreeMap<Local, usize>,
    defined: &mut BTreeSet<Local>,
) -> Result<(), Error> {
    if !defined.insert(local) {
        return Err(Error::DuplicateBinding(local));
    }
    tokens.insert(local, 1);
    Ok(())
}

fn take(local: Local, tokens: &mut BTreeMap<Local, usize>) -> Result<(), Error> {
    let count = tokens.get_mut(&local).ok_or(Error::NoToken(local))?;
    *count -= 1;
    if *count == 0 {
        tokens.remove(&local);
    }
    Ok(())
}

fn check(
    block: &Block,
    mut tokens: BTreeMap<Local, usize>,
    mut defined: BTreeSet<Local>,
) -> Result<(), Error> {
    for instruction in &block.instructions {
        match instruction {
            Instruction::Let { binding, value } => {
                if let Operation::Handle { effects, .. } = value {
                    handlers::validate(effects)?;
                }
                if let Operation::Match { pattern, .. } = value {
                    pattern_bindings(pattern)?;
                }
                if let Operation::ConstructWithDicts { tag, fields, dicts }
                | Operation::ReuseWithDicts {
                    tag, fields, dicts, ..
                } = value
                {
                    fields
                        .len()
                        .checked_add(dicts.len())
                        .ok_or(Error::ConstructorFieldOverflow)?;
                    if !dicts.is_empty() && matches!(tag.as_str(), "$tuple" | "$list" | "$match") {
                        return Err(Error::ReservedConstructorEvidence(tag.clone()));
                    }
                }
                if let Some(local) = value.borrowed()
                    && !tokens.contains_key(&local)
                {
                    return Err(Error::NoToken(local));
                }
                for local in value.consumed() {
                    take(local, &mut tokens)?;
                }
                bind(*binding, &mut tokens, &mut defined)?;
            }
            Instruction::Dup(local) => {
                *tokens.get_mut(local).ok_or(Error::NoToken(*local))? += 1;
            }
            Instruction::Drop(local) => take(*local, &mut tokens)?,
        }
    }
    match &block.exit {
        Exit::Return(local) => {
            take(*local, &mut tokens)?;
            if tokens.is_empty() {
                Ok(())
            } else {
                Err(Error::Leftover(tokens.into_iter().collect()))
            }
        }
        Exit::Branch {
            condition,
            then_,
            else_,
        } => {
            take(*condition, &mut tokens)?;
            check(then_, tokens.clone(), defined.clone())?;
            check(else_, tokens, defined)
        }
    }
}
